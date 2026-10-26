// SPDX-License-Identifier: GPL-3.0-or-later
//! Allocation-free HTTP v2 renderer and v1 compatibility adapter.
use crate::{
    CommandOutcome, Control,
    http::{self, Policy, RequestError, Route},
    management::{Action, DeviceId, Generation, Manager, Ticket},
};
use core::fmt::{self, Write};

pub const MIN_RESPONSE_CAPACITY: usize = 1024;
pub const HTML: &[u8] = include_bytes!("../../../web/control/index.html");
pub const CSS: &[u8] = include_bytes!("../../../web/control/control.css");
pub const JS: &[u8] = include_bytes!("../../../web/control/control.js");
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderError {
    BufferTooSmall,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Response {
    pub len: usize,
    pub status: u16,
}

/// One request, one response, Connection: close. Caller must serialize this with
/// its Bluetooth owner. A successful command response means queued, not executed.
pub fn handle<const N: usize>(
    request: &[u8],
    policy: Policy<'_>,
    legacy: &mut Control,
    manager: &mut Manager<N>,
    output: &mut [u8],
) -> Result<Response, RenderError> {
    // Guaranteed space for every mutating route's bounded acknowledgment. Never
    // accept a command and then discover that its response storage is too short.
    if output.len() < MIN_RESPONSE_CAPACITY {
        return Err(RenderError::BufferTooSmall);
    }
    let envelope = match http::envelope(request, policy, true) {
        Ok(value) => value,
        Err(error) => return error_response(error, output),
    };
    let mut code = 200;
    let mut content = "application/json; charset=utf-8";
    let body = match (envelope.method, envelope.path) {
        (b"GET", b"/") => {
            content = "text/html; charset=utf-8";
            Body::Static(HTML)
        }
        (b"GET", b"/control.css") => {
            content = "text/css; charset=utf-8";
            Body::Static(CSS)
        }
        (b"GET", b"/control.js") => {
            content = "text/javascript; charset=utf-8";
            Body::Static(JS)
        }
        (b"GET", b"/v2/public-status") => Body::Public(manager.snapshot()),
        (b"GET", b"/v2/status")
            if envelope.generation.is_none()
                && envelope.device.is_none()
                && envelope.approval.is_none() =>
        {
            Body::Status(manager)
        }
        (b"GET", b"/v2/devices")
            if envelope.generation.is_none()
                && envelope.device.is_none()
                && envelope.approval.is_none() =>
        {
            Body::Devices(manager)
        }
        (_, path) if path.starts_with(b"/v1/") => {
            match http::authorize_and_route(request, policy) {
                Ok(Route::Status) => Body::V1(legacy),
                Ok(Route::Command(_))
                    if manager.pending().is_some() || manager.urgent().is_some() =>
                {
                    code = 409;
                    Body::Static(b"{\"error\":\"busy\"}")
                }
                Ok(Route::Command(command)) => match legacy.submit(command) {
                    Ok(ticket) => {
                        code = 202;
                        Body::V1Accepted(ticket.id)
                    }
                    Err(_) => {
                        code = 409;
                        Body::Static(b"{\"error\":\"unavailable_or_busy\"}")
                    }
                },
                Err(error) => return error_response(error, output),
            }
        }
        (b"POST", path) => {
            let generation = envelope.generation.and_then(parse_generation);
            let device = envelope.device.and_then(parse_u64).map(DeviceId);
            let approval = envelope.approval.and_then(parse_u64);
            let action = match path {
                b"/v2/scan" => Some(Action::Scan),
                b"/v2/cancel-scan" => Some(Action::CancelScan),
                b"/v2/select" => device.map(Action::Select),
                b"/v2/pair" => Some(Action::Pair),
                b"/v2/approve" => approval.map(|token| Action::Approve { token }),
                b"/v2/reject" => approval.map(|token| Action::Reject { token }),
                b"/v2/connect" => Some(Action::Connect),
                b"/v2/start" => Some(Action::Start),
                b"/v2/stop" => Some(Action::Stop),
                b"/v2/disconnect" => Some(Action::Disconnect),
                b"/v2/forget" => Some(Action::Forget),
                _ => None,
            };
            if !matches!(
                path,
                b"/v2/scan"
                    | b"/v2/cancel-scan"
                    | b"/v2/select"
                    | b"/v2/pair"
                    | b"/v2/approve"
                    | b"/v2/reject"
                    | b"/v2/connect"
                    | b"/v2/start"
                    | b"/v2/stop"
                    | b"/v2/disconnect"
                    | b"/v2/forget"
            ) {
                return error_response(RequestError::NotFound, output);
            }
            let (Some(generation), Some(action)) = (generation, action) else {
                return error_response(RequestError::Malformed, output);
            };
            if (envelope.device.is_some() != (path == b"/v2/select"))
                || (envelope.approval.is_some() != matches!(path, b"/v2/approve" | b"/v2/reject"))
            {
                code = 400;
                Body::Static(b"{\"error\":\"invalid_command_headers\"}")
            } else {
                match if legacy.pending().is_some() {
                    Err(crate::management::Error::Busy)
                } else {
                    manager.submit(generation, action)
                } {
                    Ok(ticket) => {
                        code = 202;
                        Body::Accepted(ticket, manager.generation())
                    }
                    Err(crate::management::Error::StaleGeneration) => {
                        code = 409;
                        Body::Static(b"{\"error\":\"stale_generation\"}")
                    }
                    Err(crate::management::Error::Unavailable) => {
                        code = 503;
                        Body::Static(b"{\"error\":\"unavailable\"}")
                    }
                    Err(crate::management::Error::Busy) => {
                        code = 409;
                        Body::Static(b"{\"error\":\"busy\"}")
                    }
                    Err(_) => {
                        code = 409;
                        Body::Static(b"{\"error\":\"invalid_state\"}")
                    }
                }
            }
        }
        (b"GET", _) => return error_response(RequestError::NotFound, output),
        _ => return error_response(RequestError::UnsupportedMethod, output),
    };
    render(code, content, &body, output)
}

pub fn parse_u64(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() || bytes.len() > 20 || (bytes.len() > 1 && bytes[0] == b'0') {
        return None;
    }
    bytes.iter().try_fold(0u64, |value, byte| {
        if !byte.is_ascii_digit() {
            return None;
        }
        value.checked_mul(10)?.checked_add(u64::from(byte - b'0'))
    })
}
fn parse_generation(bytes: &[u8]) -> Option<Generation> {
    let mut parts = bytes.split(|b| *b == b':');
    let result = Generation {
        session: parse_u64(parts.next()?)?,
        revision: parse_u64(parts.next()?)?,
    };
    if parts.next().is_some() || result.session == 0 {
        None
    } else {
        Some(result)
    }
}
pub(crate) fn error_response(
    error: RequestError,
    output: &mut [u8],
) -> Result<Response, RenderError> {
    let (code, body) = match error {
        RequestError::Unauthorized => (401, b"{\"error\":\"unauthorized\"}".as_slice()),
        RequestError::CrossSite => (403, b"{\"error\":\"cross_site\"}".as_slice()),
        RequestError::NotFound => (404, b"{\"error\":\"not_found\"}".as_slice()),
        RequestError::TooLarge => (413, b"{\"error\":\"request_too_large\"}".as_slice()),
        _ => (400, b"{\"error\":\"invalid_request\"}".as_slice()),
    };
    render::<0>(
        code,
        "application/json; charset=utf-8",
        &Body::Static(body),
        output,
    )
}

enum Body<'a, const N: usize> {
    Static(&'a [u8]),
    V1(&'a Control),
    LegacyStatus(crate::Status),
    Public(crate::management::Snapshot),
    V1Accepted(u64),
    Accepted(Ticket, Generation),
    Status(&'a Manager<N>),
    Devices(&'a Manager<N>),
}
struct Sink<'a> {
    output: Option<&'a mut [u8]>,
    len: usize,
}
impl Write for Sink<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let end = self.len.checked_add(s.len()).ok_or(fmt::Error)?;
        if let Some(output) = self.output.as_deref_mut() {
            output
                .get_mut(self.len..end)
                .ok_or(fmt::Error)?
                .copy_from_slice(s.as_bytes());
        }
        self.len = end;
        Ok(())
    }
}
fn quoted(w: &mut Sink<'_>, s: &str) -> fmt::Result {
    w.write_char('"')?;
    for c in s.chars() {
        match c {
            '"' => w.write_str("\\\"")?,
            '\\' => w.write_str("\\\\")?,
            '\u{0}'..='\u{1f}' | '<' | '>' | '&' | '\u{2028}' | '\u{2029}' => {
                write!(w, "\\u{:04x}", u32::from(c))?
            }
            _ => w.write_char(c)?,
        }
    }
    w.write_char('"')
}
fn generation(w: &mut Sink<'_>, g: Generation) -> fmt::Result {
    write!(w, "\"{}:{}\"", g.session, g.revision)
}
fn pending(w: &mut Sink<'_>, owned: Option<crate::management::Owned>) -> fmt::Result {
    if let Some(o) = owned {
        write!(
            w,
            "{{\"id\":\"{}\",\"action\":\"{}\",\"claimed\":{}}}",
            o.ticket.id,
            o.ticket.action.name(),
            o.claimed
        )
    } else {
        w.write_str("null")
    }
}
impl<const N: usize> Body<'_, N> {
    fn write(&self, w: &mut Sink<'_>) -> fmt::Result {
        match self {
            Self::Static(data) => w.write_str(core::str::from_utf8(data).map_err(|_| fmt::Error)?),
            Self::V1(control) => {
                let mut bytes = [0; 512];
                let len = control.write_status(&mut bytes).map_err(|_| fmt::Error)?;
                w.write_str(core::str::from_utf8(&bytes[..len]).map_err(|_| fmt::Error)?)
            }
            Self::Public(snapshot) => {
                write!(
                    w,
                    "{{\"version\":2,\"owner_phase\":\"{}\",\"cleanup_required\":{},\"status\":",
                    snapshot.owner_phase.name(),
                    snapshot.cleanup_required
                )?;
                Body::<'_, N>::LegacyStatus(snapshot.status).write(w)?;
                w.write_str("}")
            }
            Self::LegacyStatus(status) => {
                let control = Control::new(*status);
                let mut bytes = [0; 512];
                let len = control.write_status(&mut bytes).map_err(|_| fmt::Error)?;
                w.write_str(core::str::from_utf8(&bytes[..len]).map_err(|_| fmt::Error)?)
            }
            Self::V1Accepted(id) => write!(w, "{{\"id\":\"{id}\",\"queued\":true}}"),
            Self::Accepted(ticket, g) => {
                write!(
                    w,
                    "{{\"version\":2,\"queued\":true,\"id\":\"{}\",\"generation\":",
                    ticket.id
                )?;
                generation(w, *g)?;
                w.write_str("}")
            }
            Self::Devices(manager) => {
                w.write_str("{\"version\":2,\"generation\":")?;
                generation(w, manager.generation())?;
                w.write_str(",\"devices\":[")?;
                for (i, d) in manager.devices().enumerate() {
                    if i > 0 {
                        w.write_str(",")?;
                    }
                    write!(w, "{{\"id\":\"{}\",\"name\":", d.id.0)?;
                    quoted(w, d.name.as_str())?;
                    w.write_str(",\"bonded\":")?;
                    w.write_str(match d.bonded {
                        Some(true) => "true",
                        Some(false) => "false",
                        None => "null",
                    })?;
                    w.write_str(",\"audio_sink\":")?;
                    w.write_str(match d.audio_sink {
                        Some(true) => "true",
                        Some(false) => "false",
                        None => "null",
                    })?;
                    write!(
                        w,
                        ",\"selected\":{}}}",
                        manager.snapshot().selected == Some(d.id)
                    )?;
                }
                w.write_str("]}")
            }
            Self::Status(manager) => {
                let s = manager.snapshot();
                w.write_str("{\"version\":2,\"generation\":")?;
                generation(w, manager.generation())?;
                write!(
                    w,
                    ",\"owner_phase\":\"{}\",\"cleanup_required\":{}",
                    s.owner_phase.name(),
                    s.cleanup_required
                )?;
                w.write_str(",\"status\":")?;
                Self::LegacyStatus(s.status).write(w)?;
                write!(w, ",\"scanning\":{},\"selected\":", s.scanning)?;
                if let Some(id) = s.selected {
                    write!(w, "\"{}\"", id.0)?;
                } else {
                    w.write_str("null")?;
                }
                w.write_str(",\"approval\":")?;
                if let Some(a) = s.approval {
                    write!(
                        w,
                        "{{\"token\":\"{}\",\"device\":\"{}\",\"number\":",
                        a.token, a.device.0
                    )?;
                    if let Some(n) = a.number {
                        write!(w, "{n}")?;
                    } else {
                        w.write_str("null")?;
                    }
                    w.write_str("}")?;
                } else {
                    w.write_str("null")?;
                }
                w.write_str(",\"available\":[")?;
                let mut first = true;
                for a in [
                    Action::Scan,
                    Action::CancelScan,
                    Action::Select(DeviceId(0)),
                    Action::Pair,
                    Action::Approve {
                        token: s.approval.map_or(0, |a| a.token),
                    },
                    Action::Reject {
                        token: s.approval.map_or(0, |a| a.token),
                    },
                    Action::Connect,
                    Action::Start,
                    Action::Stop,
                    Action::Disconnect,
                    Action::Forget,
                ] {
                    let available = if matches!(a, Action::Select(_)) {
                        manager
                            .devices()
                            .any(|d| manager.can_submit(Action::Select(d.id)))
                    } else {
                        manager.can_submit(a)
                    };
                    if available {
                        if !first {
                            w.write_str(",")?;
                        }
                        first = false;
                        quoted(w, a.name())?;
                    }
                }
                w.write_str("],\"pending\":")?;
                pending(w, manager.pending())?;
                w.write_str(",\"urgent\":")?;
                pending(w, manager.urgent())?;
                w.write_str(",\"completion\":")?;
                if let Some(c) = manager.completion() {
                    write!(
                        w,
                        "{{\"id\":\"{}\",\"action\":\"{}\",\"outcome\":",
                        c.ticket.id,
                        c.ticket.action.name()
                    )?;
                    match c.outcome {
                        CommandOutcome::Success => w.write_str("\"success\"")?,
                        CommandOutcome::Cancelled => w.write_str("\"cancelled\"")?,
                        CommandOutcome::Failed(code) => write!(w, "\"failed\",\"code\":{code}")?,
                    }
                    w.write_str("}")?;
                } else {
                    w.write_str("null")?;
                }
                w.write_str("}")
            }
        }
    }
}
fn headers(w: &mut Sink<'_>, code: u16, content: &str, len: usize) -> fmt::Result {
    let reason = match code {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        413 => "Content Too Large",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    };
    write!(
        w,
        "HTTP/1.1 {code} {reason}\r\nContent-Type: {content}\r\nContent-Length: {len}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'\r\n\r\n"
    )
}
fn render<const N: usize>(
    code: u16,
    content: &str,
    body: &Body<'_, N>,
    output: &mut [u8],
) -> Result<Response, RenderError> {
    let mut count = Sink {
        output: None,
        len: 0,
    };
    body.write(&mut count)
        .map_err(|_| RenderError::BufferTooSmall)?;
    let body_len = count.len;
    count.len = 0;
    headers(&mut count, code, content, body_len).map_err(|_| RenderError::BufferTooSmall)?;
    let required = count
        .len
        .checked_add(body_len)
        .ok_or(RenderError::BufferTooSmall)?;
    if output.len() < required {
        return Err(RenderError::BufferTooSmall);
    }
    let mut sink = Sink {
        output: Some(output),
        len: 0,
    };
    headers(&mut sink, code, content, body_len).map_err(|_| RenderError::BufferTooSmall)?;
    body.write(&mut sink)
        .map_err(|_| RenderError::BufferTooSmall)?;
    Ok(Response {
        len: sink.len,
        status: code,
    })
}
