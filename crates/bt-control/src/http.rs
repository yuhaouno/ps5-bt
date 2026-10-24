// SPDX-License-Identifier: GPL-3.0-or-later
//! Strict HTTP/1.1 request boundary for a future single-connection LAN server.

use crate::Command;

pub const MAX_REQUEST_BYTES: usize = 1024;
pub const MAX_HEADER_LINE_BYTES: usize = 256;
pub const MAX_HEADERS: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy<'a> {
    /// Exact Host header, including a port when the listener uses one.
    pub host: &'a [u8],
    /// Exact trusted Origin for state-changing browser requests.
    pub origin: &'a [u8],
    /// Secret bytes after `Authorization: Bearer `. Never logged or serialized.
    pub bearer_token: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Status,
    Command(Command),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestError {
    TooLarge,
    Incomplete,
    Malformed,
    UnsupportedMethod,
    NotFound,
    Unauthorized,
    CrossSite,
    BodyNotAllowed,
}

#[derive(Clone, Copy)]
struct Critical<'a> {
    host: Option<&'a [u8]>,
    authorization: Option<&'a [u8]>,
    origin: Option<&'a [u8]>,
    control: Option<&'a [u8]>,
    content_length: Option<&'a [u8]>,
    connection: Option<&'a [u8]>,
    count: usize,
}

impl<'a> Critical<'a> {
    const fn new() -> Self {
        Self {
            host: None,
            authorization: None,
            origin: None,
            control: None,
            content_length: None,
            connection: None,
            count: 0,
        }
    }
}

/// Parse one complete request. No body, pipelining, transfer coding, folded
/// headers, duplicate security headers or query string is accepted.
pub fn authorize_and_route(request: &[u8], policy: Policy<'_>) -> Result<Route, RequestError> {
    if request.len() > MAX_REQUEST_BYTES {
        return Err(RequestError::TooLarge);
    }
    if request.len() < 4 || !request.ends_with(b"\r\n\r\n") {
        return Err(RequestError::Incomplete);
    }
    for (index, byte) in request.iter().copied().enumerate() {
        if (byte == b'\r' && request.get(index + 1) != Some(&b'\n'))
            || (byte == b'\n' && index.checked_sub(1).and_then(|i| request.get(i)) != Some(&b'\r'))
        {
            return Err(RequestError::Malformed);
        }
    }
    if request
        .iter()
        .any(|byte| !matches!(byte, b'\t' | b'\r' | b'\n' | 0x20..=0x7e))
    {
        return Err(RequestError::Malformed);
    }
    let mut lines = request[..request.len() - 2].split(|byte| *byte == b'\n');
    let request_line = trim_cr(lines.next().ok_or(RequestError::Malformed)?);
    let (method, path) = parse_request_line(request_line)?;
    let mut headers = Critical::new();
    for raw_line in lines {
        let line = trim_cr(raw_line);
        if line.is_empty() {
            continue;
        }
        if line.len() > MAX_HEADER_LINE_BYTES || matches!(line.first(), Some(b' ' | b'\t')) {
            return Err(RequestError::Malformed);
        }
        headers.count += 1;
        if headers.count > MAX_HEADERS {
            return Err(RequestError::TooLarge);
        }
        let separator = line
            .iter()
            .position(|byte| *byte == b':')
            .ok_or(RequestError::Malformed)?;
        let name = &line[..separator];
        let value = trim_horizontal(&line[separator + 1..]);
        if name.is_empty() || name.iter().any(|byte| !is_header_name(*byte)) {
            return Err(RequestError::Malformed);
        }
        if eq_ignore_ascii_case(name, b"transfer-encoding") {
            return Err(RequestError::BodyNotAllowed);
        }
        if eq_ignore_ascii_case(name, b"host") {
            set_once(&mut headers.host, value)?;
        } else if eq_ignore_ascii_case(name, b"authorization") {
            set_once(&mut headers.authorization, value)?;
        } else if eq_ignore_ascii_case(name, b"origin") {
            set_once(&mut headers.origin, value)?;
        } else if eq_ignore_ascii_case(name, b"x-ps5-bt-control") {
            set_once(&mut headers.control, value)?;
        } else if eq_ignore_ascii_case(name, b"content-length") {
            set_once(&mut headers.content_length, value)?;
        } else if eq_ignore_ascii_case(name, b"connection") {
            set_once(&mut headers.connection, value)?;
        }
    }
    if headers.host != Some(policy.host)
        || headers
            .connection
            .is_some_and(|value| !eq_ignore_ascii_case(value, b"close"))
    {
        return Err(RequestError::Malformed);
    }
    let authorization = headers
        .authorization
        .and_then(|value| value.strip_prefix(b"Bearer "))
        .ok_or(RequestError::Unauthorized)?;
    if policy.bearer_token.is_empty() || !constant_time_equal(authorization, policy.bearer_token) {
        return Err(RequestError::Unauthorized);
    }
    if headers.content_length.is_some_and(|value| value != b"0") {
        return Err(RequestError::BodyNotAllowed);
    }

    match (method, path) {
        (b"GET", b"/v1/status") => Ok(Route::Status),
        (b"POST", b"/v1/connect" | b"/v1/start" | b"/v1/stop") => {
            if headers.origin != Some(policy.origin) || headers.control != Some(b"1") {
                return Err(RequestError::CrossSite);
            }
            let command = match path {
                b"/v1/connect" => Command::Connect,
                b"/v1/start" => Command::Start,
                _ => Command::Stop,
            };
            Ok(Route::Command(command))
        }
        (b"GET" | b"POST", _) => Err(RequestError::NotFound),
        _ => Err(RequestError::UnsupportedMethod),
    }
}

fn parse_request_line(line: &[u8]) -> Result<(&[u8], &[u8]), RequestError> {
    let mut parts = line.split(|byte| *byte == b' ');
    let method = parts.next().ok_or(RequestError::Malformed)?;
    let path = parts.next().ok_or(RequestError::Malformed)?;
    let version = parts.next().ok_or(RequestError::Malformed)?;
    if parts.next().is_some()
        || method.is_empty()
        || path.is_empty()
        || version != b"HTTP/1.1"
        || path.contains(&b'?')
    {
        return Err(RequestError::Malformed);
    }
    Ok((method, path))
}

fn trim_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

fn trim_horizontal(mut value: &[u8]) -> &[u8] {
    while matches!(value.first(), Some(b' ' | b'\t')) {
        value = &value[1..];
    }
    while matches!(value.last(), Some(b' ' | b'\t')) {
        value = &value[..value.len() - 1];
    }
    value
}

fn set_once<'a>(slot: &mut Option<&'a [u8]>, value: &'a [u8]) -> Result<(), RequestError> {
    if slot.replace(value).is_some() {
        return Err(RequestError::Malformed);
    }
    Ok(())
}

fn is_header_name(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'..=b'\'' | b'*' | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
        )
}

fn eq_ignore_ascii_case(left: &[u8], right: &[u8]) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (left, right) in left.iter().zip(right) {
        difference |= left ^ right;
    }
    difference == 0
}
