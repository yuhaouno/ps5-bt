// SPDX-License-Identifier: GPL-3.0-or-later
use bt_control::{
    http::{Policy, RequestError},
    management::*,
    service, *,
};
const POLICY: Policy<'_> = Policy {
    host: b"console:8080",
    origin: b"http://console:8080",
    bearer_token: b"synthetic-secret-token",
};
fn status() -> Status {
    Status {
        transport: Capability::Ready,
        capture: Capability::Ready,
        codec: Capability::Ready,
        playback: Capability::Ready,
        link: LinkState::Disconnected,
        stream: StreamState::Stopped,
        last_error: None,
        revision: 0,
    }
}
fn snapshot() -> Snapshot {
    Snapshot {
        status: status(),
        owner_phase: OwnerPhase::Idle,
        cleanup_required: false,
        scanning: false,
        selected: None,
        approval: None,
        available: Actions::none()
            .with(Action::Scan)
            .with(Action::Select(DeviceId(1)))
            .with(Action::Connect),
    }
}
fn request(method: &str, path: &str, extra: &str, auth: bool) -> Vec<u8> {
    format!(
        "{method} {path} HTTP/1.1\r\nHost: console:8080\r\n{}{}\r\n",
        if auth {
            "Authorization: Bearer synthetic-secret-token\r\n"
        } else {
            ""
        },
        extra
    )
    .into_bytes()
}
fn command(path: &str, g: Generation, extra: &str) -> Vec<u8> {
    request(
        "POST",
        path,
        &format!(
            "Origin: http://console:8080\r\nX-PS5-BT-Control: 1\r\nX-PS5-BT-Generation: {}:{}\r\n{}",
            g.session, g.revision, extra
        ),
        true,
    )
}
fn handle(request: &[u8], legacy: &mut Control, m: &mut Manager<2>) -> (u16, String) {
    let mut output = [0; 16384];
    let r = service::handle(request, POLICY, legacy, m, &mut output).unwrap();
    (
        r.status,
        String::from_utf8(output[..r.len].to_vec()).unwrap(),
    )
}
#[test]
fn protected_devices_escape_labels_and_public_status_stays_redacted() {
    let mut m = Manager::<2>::new(11, snapshot()).unwrap();
    let mut legacy = Control::new(status());
    let mut s = snapshot();
    s.selected = Some(DeviceId(9));
    s.approval = Some(Approval {
        token: 987654,
        device: DeviceId(9),
        number: Some(123456),
    });
    s.owner_phase = OwnerPhase::RequiresCleanup;
    s.cleanup_required = true;
    m.observe(
        s,
        &[Device {
            id: DeviceId(9),
            name: Name::new("<script>\"\\\n雪").unwrap(),
            bonded: None,
            audio_sink: Some(true),
        }],
    )
    .unwrap();
    assert_eq!(
        handle(
            &request("GET", "/v2/devices", "", false),
            &mut legacy,
            &mut m
        )
        .0,
        401
    );
    let (_, listing) = handle(
        &request("GET", "/v2/devices", "", true),
        &mut legacy,
        &mut m,
    );
    assert!(listing.contains("\\u003cscript\\u003e\\\"\\\\\\u000a雪"));
    assert!(listing.contains("\"bonded\":null"));
    let (_, protected) = handle(&request("GET", "/v2/status", "", true), &mut legacy, &mut m);
    assert!(protected.contains("987654"));
    let (_, public) = handle(
        &request("GET", "/v2/public-status", "", false),
        &mut legacy,
        &mut m,
    );
    assert!(public.contains("requires-cleanup"));
    assert!(!public.contains("987654"));
    assert!(!public.contains("selected"));
    assert!(!public.contains("device"));
    assert!(!public.contains("雪"));
    for header in [
        "Cache-Control: no-store",
        "X-Content-Type-Options: nosniff",
        "frame-ancestors 'none'",
        "Referrer-Policy: no-referrer",
        "Connection: close",
    ] {
        assert!(public.contains(header));
    }
    let parts: Vec<_> = public.split("\r\n\r\n").collect();
    let length = parts[0]
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert_eq!(length, parts[1].len());
}
#[test]
fn stale_ui_and_security_failures_never_submit_commands() {
    let mut m = Manager::<2>::new(11, snapshot()).unwrap();
    let mut legacy = Control::new(status());
    let old = m.generation();
    let mut changed = snapshot();
    changed.status.last_error = Some(-7);
    m.observe(changed, &[]).unwrap();
    assert_eq!(
        handle(&command("/v2/scan", old, ""), &mut legacy, &mut m).0,
        409
    );
    assert!(m.pending().is_none());
    let current = m.generation();
    let good = command("/v2/scan", current, "");
    for bad in [
        command("/v2/scan", current, "X-PS5-BT-Generation: 11:1\r\n"),
        request("POST", "/v2/scan", "X-PS5-BT-Control: 1\r\n", true),
        command("/v2/scan", current, "Content-Length: 1\r\n"),
        command("/v2/scan", current, "X-PS5-BT-Device: 1\r\n"),
    ] {
        assert_ne!(handle(&bad, &mut legacy, &mut m).0, 202);
        assert!(m.pending().is_none());
    }
    assert_eq!(handle(&good, &mut legacy, &mut m).0, 202);
    assert_eq!(m.pending().unwrap().ticket.action, Action::Scan);
}
#[test]
fn v1_compatibility_and_cross_version_serialization() {
    let mut m = Manager::<2>::new(11, snapshot()).unwrap();
    let mut legacy = Control::new(status());
    let v1 = request(
        "POST",
        "/v1/connect",
        "Origin: http://console:8080\r\nX-PS5-BT-Control: 1\r\n",
        true,
    );
    assert_eq!(handle(&v1, &mut legacy, &mut m).0, 202);
    assert_eq!(
        handle(
            &command("/v2/scan", m.generation(), ""),
            &mut legacy,
            &mut m
        )
        .0,
        409
    );
    let pending = legacy.pending().unwrap();
    legacy.finish(pending, CommandOutcome::Failed(-7)).unwrap();
    assert_eq!(
        handle(
            &command("/v2/scan", m.generation(), ""),
            &mut legacy,
            &mut m
        )
        .0,
        202
    );
    assert_eq!(handle(&v1, &mut legacy, &mut m).0, 409);
    assert_eq!(
        http::authorize_and_route(&request("GET", "/v2/status", "", true), POLICY),
        Err(RequestError::NotFound)
    );
}
#[test]
fn short_output_cannot_accept_and_requests_cannot_smuggle_header_shaped_bodies() {
    let mut m = Manager::<2>::new(11, snapshot()).unwrap();
    let mut legacy = Control::new(status());
    let req = command("/v2/scan", m.generation(), "");
    let mut short = [0xa5; 1023];
    assert_eq!(
        service::handle(&req, POLICY, &mut legacy, &mut m, &mut short),
        Err(service::RenderError::BufferTooSmall)
    );
    assert_eq!(short, [0xa5; 1023]);
    assert!(m.pending().is_none());
    let mut smuggled = request("GET", "/v2/status", "", true);
    smuggled.extend(b"X-Body: x\r\n\r\n");
    assert_eq!(handle(&smuggled, &mut legacy, &mut m).0, 400);
    let mut pipeline = req.clone();
    pipeline.extend(&req);
    assert_eq!(handle(&pipeline, &mut legacy, &mut m).0, 400);
    assert!(m.pending().is_none());
    for invalid in [
        b"01".as_slice(),
        b"18446744073709551616",
        b"+1",
        b"1:2",
        b"",
    ] {
        assert_eq!(service::parse_u64(invalid), None);
    }
}
#[test]
fn every_asset_is_concrete_and_public_without_inline_script_or_token_storage() {
    let mut m = Manager::<2>::new(11, snapshot()).unwrap();
    let mut legacy = Control::new(status());
    for path in ["/", "/control.css", "/control.js"] {
        let (code, response) = handle(&request("GET", path, "", false), &mut legacy, &mut m);
        assert_eq!(code, 200);
        assert!(response.contains("default-src 'none'"));
    }
    let js = core::str::from_utf8(service::JS).unwrap();
    assert!(!js.contains("localStorage"));
    assert!(!js.contains("sessionStorage"));
    assert!(!js.contains("innerHTML"));
    assert!(js.contains("textContent"));
    assert!(js.contains("X-PS5-BT-Generation"));
    let html = core::str::from_utf8(service::HTML).unwrap();
    assert!(!html.contains("onclick="));
    assert!(html.contains("src=\"/control.js\""));
}
#[test]
fn every_management_route_has_exact_typed_header_contract() {
    for (path, expected, extra) in [
        ("scan", Action::Scan, ""),
        ("cancel-scan", Action::CancelScan, ""),
        (
            "select",
            Action::Select(DeviceId(1)),
            "X-PS5-BT-Device: 1\r\n",
        ),
        ("pair", Action::Pair, ""),
        (
            "approve",
            Action::Approve { token: 21 },
            "X-PS5-BT-Approval: 21\r\n",
        ),
        (
            "reject",
            Action::Reject { token: 21 },
            "X-PS5-BT-Approval: 21\r\n",
        ),
        ("connect", Action::Connect, ""),
        ("start", Action::Start, ""),
        ("stop", Action::Stop, ""),
        ("disconnect", Action::Disconnect, ""),
        ("forget", Action::Forget, ""),
    ] {
        let mut s = snapshot();
        s.selected = Some(DeviceId(1));
        s.approval = Some(Approval {
            token: 21,
            device: DeviceId(1),
            number: None,
        });
        s.available = s.available.with(expected);
        let mut m = Manager::<2>::new(11, snapshot()).unwrap();
        m.observe(
            s,
            &[Device {
                id: DeviceId(1),
                name: Name::new("Headset").unwrap(),
                bonded: Some(false),
                audio_sink: Some(true),
            }],
        )
        .unwrap();
        let mut legacy = Control::new(status());
        assert_eq!(
            handle(
                &command(&format!("/v2/{path}"), m.generation(), extra),
                &mut legacy,
                &mut m
            )
            .0,
            202,
            "{path}"
        );
        assert_eq!(m.pending().unwrap().ticket.action, expected);
    }
}

#[test]
fn ordinary_browser_keep_alive_preference_does_not_enable_pipelining_or_upgrade() {
    let mut m = Manager::<2>::new(11, snapshot()).unwrap();
    let mut legacy = Control::new(status());
    for path in ["/", "/control.js", "/v2/status", "/v2/devices"] {
        let (code, response) = handle(
            &request("GET", path, "Connection: keep-alive\r\n", true),
            &mut legacy,
            &mut m,
        );
        assert_eq!(code, 200);
        assert!(response.contains("Connection: close\r\n"));
    }
    assert_eq!(
        handle(
            &command("/v2/scan", m.generation(), "Connection: Keep-Alive\r\n"),
            &mut legacy,
            &mut m
        )
        .0,
        202
    );
    for connection in ["Upgrade", "keep-alive, upgrade", "close, keep-alive"] {
        assert_eq!(
            handle(
                &request(
                    "GET",
                    "/v2/status",
                    &format!("Connection: {connection}\r\n"),
                    true
                ),
                &mut legacy,
                &mut m
            )
            .0,
            400
        );
    }
    assert_eq!(
        http::authorize_and_route(
            &request("GET", "/v1/status", "Connection: keep-alive\r\n", true),
            POLICY
        ),
        Err(RequestError::Malformed)
    );
}
