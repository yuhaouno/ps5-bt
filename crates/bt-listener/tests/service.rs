// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "native-posix")]
use bt_control::{
    http::Policy,
    management::{Action, Actions, Manager, OwnerPhase, Snapshot},
    pump::Cause,
    *,
};
use bt_listener::{
    BindConfig, Listener, ListenerStorage, ServiceConfig, ServiceState, ServiceStop,
};
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpStream},
    time::{Duration, Instant},
};
const POLICY: Policy<'_> = Policy {
    host: b"console",
    origin: b"http://console",
    bearer_token: b"synthetic-secret-token",
};
fn status() -> Status {
    Status {
        transport: Capability::Unavailable,
        capture: Capability::Unavailable,
        codec: Capability::Unavailable,
        playback: Capability::Unavailable,
        link: LinkState::Disconnected,
        stream: StreamState::Stopped,
        last_error: None,
        revision: 0,
    }
}
fn manager() -> Manager<1> {
    Manager::new(
        7,
        Snapshot {
            status: status(),
            owner_phase: OwnerPhase::Idle,
            cleanup_required: false,
            scanning: false,
            selected: None,
            approval: None,
            available: Actions::none().with(Action::Scan),
        },
    )
    .unwrap()
}
fn config(max_connections: u32) -> ServiceConfig {
    ServiceConfig {
        request_timeout_us: 100,
        max_connections,
        deadline_us: Some(100_000),
    }
}
fn connect(listener: &Listener<'_, 128>) -> TcpStream {
    let peer = TcpStream::connect_timeout(
        &SocketAddrV4::new(Ipv4Addr::LOCALHOST, listener.local_port()).into(),
        Duration::from_secs(2),
    )
    .unwrap();
    peer.set_nonblocking(true).unwrap();
    peer
}
fn request(method: &str, route: &str, bearer: &str) -> Vec<u8> {
    let generation = if method == "POST" {
        "X-PS5-BT-Generation: 7:0\r\n"
    } else {
        ""
    };
    format!("{method} {route} HTTP/1.1\r\nHost: console\r\nAuthorization: Bearer {bearer}\r\nOrigin: http://console\r\nX-PS5-BT-Control: 1\r\n{generation}\r\n").into_bytes()
}
fn accept_service(
    service: &mut ServiceState<'_>,
    listener: &mut Listener<'_, 128>,
    legacy: &mut Control,
    manager: &mut Manager<1>,
    now: u64,
) {
    let start = Instant::now();
    for _ in 0..100_000 {
        service
            .step(listener, now, POLICY, legacy, manager)
            .unwrap();
        if service.snapshot().active {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(2));
        std::thread::yield_now();
    }
    panic!("finite accept test budget");
}

fn exchange(
    service: &mut ServiceState<'_>,
    listener: &mut Listener<'_, 128>,
    manager: &mut Manager<1>,
    legacy: &mut Control,
    now: u64,
    request: &[u8],
) -> Vec<u8> {
    let mut peer = connect(listener);
    peer.write_all(request).unwrap();
    let mut response = Vec::new();
    let start = Instant::now();
    for _ in 0..100_000 {
        service
            .step(listener, now, POLICY, legacy, manager)
            .unwrap();
        let mut bytes = [0; 1024];
        match peer.read(&mut bytes) {
            Ok(0) => return response,
            Ok(n) => response.extend_from_slice(&bytes[..n]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("loopback receive: {error}"),
        }
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "finite test deadline"
        );
        std::thread::yield_now();
    }
    panic!("finite test attempt budget")
}
fn listener(storage: &mut ListenerStorage<128>) -> Listener<'_, 128> {
    Listener::bind(
        storage,
        BindConfig {
            address: [127, 0, 0, 1],
            port: 0,
        },
    )
    .unwrap()
}
#[test]
fn successive_authenticated_clients_reaccept_after_rejection_and_stop_at_total_budget() {
    let mut storage = ListenerStorage::new();
    let mut listener = listener(&mut storage);
    let (mut request_buffer, mut response_buffer) = ([0; 1024], [0; 16384]);
    let mut service =
        ServiceState::new(config(3), &mut request_buffer, &mut response_buffer).unwrap();
    let (mut manager, mut legacy) = (manager(), Control::new(status()));
    assert!(
        !service
            .step(&mut listener, 0, POLICY, &mut legacy, &mut manager)
            .unwrap()
            .progressed
    );
    for (bearer, expected) in [
        ("wrong", b"HTTP/1.1 401".as_slice()),
        ("synthetic-secret-token", b"HTTP/1.1 200"),
        ("synthetic-secret-token", b"HTTP/1.1 200"),
    ] {
        let response = exchange(
            &mut service,
            &mut listener,
            &mut manager,
            &mut legacy,
            1,
            &request("GET", "/v2/status", bearer),
        );
        assert!(response.starts_with(expected), "{response:?}");
        assert!(!service.snapshot().active);
        assert!(manager.pending().is_none());
    }
    assert!(
        service
            .step(&mut listener, 1, POLICY, &mut legacy, &mut manager)
            .unwrap()
            .stopped
    );
    assert_eq!(
        service.snapshot().stop_reason,
        Some(ServiceStop::ConnectionBudget)
    );
    assert_eq!(
        (service.snapshot().accepted, service.snapshot().completed),
        (3, 3)
    );
    service.cancel(&mut listener).unwrap();
    listener.close().unwrap();
    drop(service);
    assert_eq!(request_buffer, [0; 1024]);
    assert_eq!(response_buffer, [0; 16384]);
}
#[test]
fn partial_header_fixed_timeout_is_retained_then_next_client_works() {
    let mut storage = ListenerStorage::new();
    let mut listener = listener(&mut storage);
    let (mut rx, mut tx) = ([0; 1024], [0; 16384]);
    let mut service = ServiceState::new(config(3), &mut rx, &mut tx).unwrap();
    let (mut manager, mut legacy) = (manager(), Control::new(status()));
    let mut peer = connect(&listener);
    peer.write_all(b"POST /v2/scan HTTP/1.1\r\nHost: ").unwrap();
    accept_service(&mut service, &mut listener, &mut legacy, &mut manager, 10);
    let wall = Instant::now();
    let mut received = false;
    for _ in 0..100_000 {
        if service
            .step(&mut listener, 20, POLICY, &mut legacy, &mut manager)
            .unwrap()
            .progressed
        {
            received = true;
            break;
        }
        assert!(wall.elapsed() < Duration::from_secs(2));
        std::thread::yield_now();
    }
    assert!(received, "partial request readiness budget");
    service
        .step(&mut listener, 109, POLICY, &mut legacy, &mut manager)
        .unwrap();
    let progress = service
        .step(&mut listener, 110, POLICY, &mut legacy, &mut manager)
        .unwrap();
    assert!(progress.progressed);
    assert!(!progress.stopped);
    assert_eq!(
        service.snapshot().last_failure.unwrap().cause,
        Cause::Timeout
    );
    assert!(manager.pending().is_none());
    peer.set_nonblocking(false).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
    let response = exchange(
        &mut service,
        &mut listener,
        &mut manager,
        &mut legacy,
        111,
        &request("GET", "/v2/status", "synthetic-secret-token"),
    );
    assert!(response.starts_with(b"HTTP/1.1 200"));
    assert_eq!(
        (
            service.snapshot().accepted,
            service.snapshot().completed,
            service.snapshot().failed
        ),
        (2, 1, 1)
    );
    assert_eq!(
        service.snapshot().last_failure.unwrap().cause,
        Cause::Timeout
    );
    service.cancel(&mut listener).unwrap();
    listener.close().unwrap();
}
#[test]
fn stop_precedes_ready_mutating_request_and_forbids_all_later_accepts() {
    let mut storage = ListenerStorage::new();
    let mut listener = listener(&mut storage);
    let (mut rx, mut tx) = ([0; 1024], [0; 16384]);
    let mut service = ServiceState::new(config(3), &mut rx, &mut tx).unwrap();
    let (mut manager, mut legacy) = (manager(), Control::new(status()));
    let mut peer = connect(&listener);
    peer.write_all(&request("POST", "/v2/scan", "synthetic-secret-token"))
        .unwrap();
    accept_service(&mut service, &mut listener, &mut legacy, &mut manager, 1); // accept only
    let _queued = connect(&listener);
    service.request_stop();
    assert!(
        service
            .step(&mut listener, 1, POLICY, &mut legacy, &mut manager)
            .unwrap()
            .stopped
    );
    assert!(manager.pending().is_none());
    assert_eq!(
        service.snapshot().last_client.unwrap().unwrap_err().cause,
        Cause::Cancelled
    );
    for _ in 0..3 {
        service
            .step(&mut listener, 1, POLICY, &mut legacy, &mut manager)
            .unwrap();
        service.cancel(&mut listener).unwrap();
    }
    assert_eq!(service.snapshot().accepted, 1);
    listener.close().unwrap();
}
#[test]
fn service_total_deadline_and_clock_failure_cancel_owned_client_before_any_request() {
    for regressed in [false, true] {
        let mut storage = ListenerStorage::new();
        let mut listener = listener(&mut storage);
        let (mut rx, mut tx) = ([0; 1024], [0; 16384]);
        let mut cfg = config(3);
        cfg.deadline_us = Some(20);
        let mut service = ServiceState::new(cfg, &mut rx, &mut tx).unwrap();
        let (mut manager, mut legacy) = (manager(), Control::new(status()));
        let _peer = connect(&listener);
        service
            .step(&mut listener, 10, POLICY, &mut legacy, &mut manager)
            .unwrap();
        let error = service
            .step(
                &mut listener,
                if regressed { 9 } else { 20 },
                POLICY,
                &mut legacy,
                &mut manager,
            )
            .unwrap_err();
        assert_eq!(
            error.cause,
            if regressed {
                Cause::ClockRegression
            } else {
                Cause::Timeout
            }
        );
        assert_eq!(error.cleanup, None);
        assert!(service.snapshot().stopped);
        assert!(!service.snapshot().active);
        assert!(manager.pending().is_none());
        listener.close().unwrap();
    }
}
#[test]
fn listener_still_owns_abandoned_managed_client_until_explicit_factory_close() {
    let mut storage = ListenerStorage::new();
    let mut listener = listener(&mut storage);
    let (mut rx, mut tx) = ([0; 1024], [0; 16384]);
    let mut service = ServiceState::new(config(1), &mut rx, &mut tx).unwrap();
    let (mut manager, mut legacy) = (manager(), Control::new(status()));
    let mut peer = connect(&listener);
    accept_service(&mut service, &mut listener, &mut legacy, &mut manager, 0);
    drop(service);
    listener.close().unwrap();
    peer.set_nonblocking(false).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
}

#[test]
fn response_timeout_retains_admitted_command_while_listener_serves_another_client() {
    let mut storage = ListenerStorage::new();
    let mut listener = listener(&mut storage);
    let (mut rx, mut tx) = ([0; 1024], [0; 16384]);
    let mut service = ServiceState::new(config(3), &mut rx, &mut tx).unwrap();
    let (mut manager, mut legacy) = (manager(), Control::new(status()));
    let mut peer = connect(&listener);
    peer.write_all(&request("POST", "/v2/scan", "synthetic-secret-token"))
        .unwrap();
    accept_service(&mut service, &mut listener, &mut legacy, &mut manager, 1);
    let wall = Instant::now();
    for _ in 0..100_000 {
        service
            .step(&mut listener, 1, POLICY, &mut legacy, &mut manager)
            .unwrap();
        if manager.pending().is_some() {
            break;
        }
        assert!(wall.elapsed() < Duration::from_secs(2));
        std::thread::yield_now();
    }
    let queued = manager.pending().unwrap();
    assert_eq!(queued.ticket.action, Action::Scan);
    service
        .step(&mut listener, 101, POLICY, &mut legacy, &mut manager)
        .unwrap();
    assert_eq!(
        service.snapshot().last_failure.unwrap().cause,
        Cause::Timeout
    );
    assert_eq!(manager.pending(), Some(queued));
    let response = exchange(
        &mut service,
        &mut listener,
        &mut manager,
        &mut legacy,
        102,
        &request("GET", "/v2/status", "synthetic-secret-token"),
    );
    assert!(response.starts_with(b"HTTP/1.1 200"));
    assert_eq!(manager.pending(), Some(queued));
    service.cancel(&mut listener).unwrap();
    assert_eq!(manager.pending(), Some(queued));
    listener.close().unwrap();
}
