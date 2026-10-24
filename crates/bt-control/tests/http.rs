// SPDX-License-Identifier: GPL-3.0-or-later

use bt_control::{
    Command,
    http::{
        MAX_HEADER_LINE_BYTES, MAX_REQUEST_BYTES, Policy, RequestError, Route, authorize_and_route,
    },
};

const POLICY: Policy<'_> = Policy {
    host: b"192.168.1.111:8080",
    origin: b"http://192.168.1.111:8080",
    bearer_token: b"test-token-not-a-real-secret",
};

fn status() -> Vec<u8> {
    b"GET /v1/status HTTP/1.1\r\nHost: 192.168.1.111:8080\r\nAuthorization: Bearer test-token-not-a-real-secret\r\nConnection: close\r\n\r\n".to_vec()
}

fn command(path: &str) -> Vec<u8> {
    format!(
        "POST {path} HTTP/1.1\r\nHost: 192.168.1.111:8080\r\nAuthorization: Bearer test-token-not-a-real-secret\r\nOrigin: http://192.168.1.111:8080\r\nX-PS5-BT-Control: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .into_bytes()
}

#[test]
fn routes_the_only_four_authenticated_requests() {
    assert_eq!(authorize_and_route(&status(), POLICY), Ok(Route::Status));
    assert_eq!(
        authorize_and_route(&command("/v1/connect"), POLICY),
        Ok(Route::Command(Command::Connect))
    );
    assert_eq!(
        authorize_and_route(&command("/v1/start"), POLICY),
        Ok(Route::Command(Command::Start))
    );
    assert_eq!(
        authorize_and_route(&command("/v1/stop"), POLICY),
        Ok(Route::Command(Command::Stop))
    );
}

#[test]
fn rejects_unauthorized_cross_site_and_body_bearing_commands() {
    let mut request = command("/v1/start");
    let needle = b"test-token-not-a-real-secret";
    let offset = request
        .windows(needle.len())
        .position(|w| w == needle)
        .unwrap();
    request[offset] ^= 1;
    assert_eq!(
        authorize_and_route(&request, POLICY),
        Err(RequestError::Unauthorized)
    );

    let cross_site = command("/v1/start")
        .windows(b"Origin: http://192.168.1.111:8080\r\n".len())
        .position(|window| window == b"Origin: http://192.168.1.111:8080\r\n")
        .unwrap();
    let mut missing_origin = command("/v1/start");
    missing_origin.drain(cross_site..cross_site + b"Origin: http://192.168.1.111:8080\r\n".len());
    assert_eq!(
        authorize_and_route(&missing_origin, POLICY),
        Err(RequestError::CrossSite)
    );

    let body = b"POST /v1/start HTTP/1.1\r\nHost: 192.168.1.111:8080\r\nAuthorization: Bearer test-token-not-a-real-secret\r\nOrigin: http://192.168.1.111:8080\r\nX-PS5-BT-Control: 1\r\nContent-Length: 1\r\n\r\n";
    assert_eq!(
        authorize_and_route(body, POLICY),
        Err(RequestError::BodyNotAllowed)
    );
}

#[test]
fn rejects_ambiguous_or_smuggled_requests() {
    let duplicate = b"GET /v1/status HTTP/1.1\r\nHost: 192.168.1.111:8080\r\nHost: attacker\r\nAuthorization: Bearer test-token-not-a-real-secret\r\n\r\n";
    assert_eq!(
        authorize_and_route(duplicate, POLICY),
        Err(RequestError::Malformed)
    );
    let transfer = b"POST /v1/stop HTTP/1.1\r\nHost: 192.168.1.111:8080\r\nAuthorization: Bearer test-token-not-a-real-secret\r\nOrigin: http://192.168.1.111:8080\r\nX-PS5-BT-Control: 1\r\nTransfer-Encoding: chunked\r\n\r\n";
    assert_eq!(
        authorize_and_route(transfer, POLICY),
        Err(RequestError::BodyNotAllowed)
    );
    let mut query = status();
    query.splice(4..14, b"/v1/status?x=1".iter().copied());
    assert_eq!(
        authorize_and_route(&query, POLICY),
        Err(RequestError::Malformed)
    );
    let bare_lf = b"GET /v1/status HTTP/1.1\r\nHost: 192.168.1.111:8080\nAuthorization: Bearer test-token-not-a-real-secret\r\n\r\n";
    assert_eq!(
        authorize_and_route(bare_lf, POLICY),
        Err(RequestError::Malformed)
    );
}

#[test]
fn request_and_line_limits_are_enforced() {
    let oversized = vec![b'a'; MAX_REQUEST_BYTES + 1];
    assert_eq!(
        authorize_and_route(&oversized, POLICY),
        Err(RequestError::TooLarge)
    );
    let long_value = "a".repeat(MAX_HEADER_LINE_BYTES);
    let request = format!(
        "GET /v1/status HTTP/1.1\r\nHost: 192.168.1.111:8080\r\nAuthorization: Bearer test-token-not-a-real-secret\r\nX:{long_value}\r\n\r\n"
    );
    assert_eq!(
        authorize_and_route(request.as_bytes(), POLICY),
        Err(RequestError::Malformed)
    );
}
