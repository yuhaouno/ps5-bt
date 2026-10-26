// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "native-posix")]
use bt_control::{
    http::Policy,
    management::{Action, Actions, Manager, OwnerPhase, Snapshot},
    pump::{ConnectionPump, IoError, Progress, SocketIo},
    *,
};
use bt_listener::{BindConfig, Listener, ListenerStorage};
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpStream},
    time::{Duration, Instant},
};
fn config() -> BindConfig {
    BindConfig {
        address: [127, 0, 0, 1],
        port: 0,
    }
}
fn connect(port: u16) -> TcpStream {
    TcpStream::connect_timeout(
        &SocketAddrV4::new(Ipv4Addr::LOCALHOST, port).into(),
        Duration::from_secs(2),
    )
    .unwrap()
}
fn accepted<T>(
    listener: &mut Listener<'_, 128>,
    f: impl FnOnce(bt_listener::Client<'_, '_, 128>) -> T,
) -> T {
    let start = Instant::now();
    for _ in 0..100000 {
        if let Some(client) = listener.accept().unwrap() {
            return f(client);
        }
        assert!(start.elapsed() < Duration::from_secs(2), "accept deadline");
        std::thread::yield_now();
    }
    panic!("accept attempt limit");
}
#[test]
fn real_loopback_accept_read_write_eof_and_fresh_client_reuse() {
    let mut storage = ListenerStorage::<128>::new();
    let mut listener = Listener::bind(&mut storage, config()).unwrap();
    let port = listener.local_port();
    assert_ne!(port, 0);
    assert!(listener.accept().unwrap().is_none());
    for _ in 0..2 {
        let mut peer = connect(port);
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        accepted(&mut listener, |mut client| {
            assert_eq!(client.read(&mut [0; 8]), Err(IoError::WouldBlock));
            peer.write_all(b"request").unwrap();
            let mut out = [0; 16];
            let start = Instant::now();
            let count = loop {
                match client.read(&mut out) {
                    Ok(n) => break n,
                    Err(IoError::WouldBlock) if start.elapsed() < Duration::from_secs(2) => {
                        std::thread::yield_now()
                    }
                    other => panic!("read: {other:?}"),
                }
            };
            assert_eq!(&out[..count], b"request");
            assert_eq!(client.write(b"reply"), Ok(5));
            let mut reply = [0; 5];
            peer.read_exact(&mut reply).unwrap();
            assert_eq!(&reply, b"reply");
            peer.shutdown(std::net::Shutdown::Write).unwrap();
            let start = Instant::now();
            loop {
                match client.read(&mut out) {
                    Ok(0) => break,
                    Err(IoError::WouldBlock) if start.elapsed() < Duration::from_secs(2) => {
                        std::thread::yield_now()
                    }
                    other => panic!("EOF: {other:?}"),
                }
            }
            client.close().unwrap();
            client.close().unwrap();
            assert!(matches!(client.write(b"closed"), Err(IoError::Failed(_))));
        });
    }
    listener.close().unwrap();
}
#[test]
fn actual_listener_drives_authenticated_http_pump_and_closes_socket() {
    let mut storage = ListenerStorage::<128>::new();
    let mut listener = Listener::bind(&mut storage, config()).unwrap();
    let port = listener.local_port();
    let mut peer = connect(port);
    peer.set_nonblocking(true).unwrap();
    let status = Status {
        transport: Capability::Ready,
        capture: Capability::Ready,
        codec: Capability::Ready,
        playback: Capability::Ready,
        link: LinkState::Disconnected,
        stream: StreamState::Stopped,
        last_error: None,
        revision: 0,
    };
    let mut manager = Manager::<1>::new(
        9,
        Snapshot {
            status,
            owner_phase: OwnerPhase::Idle,
            cleanup_required: false,
            scanning: false,
            selected: None,
            approval: None,
            available: Actions::none().with(Action::Scan),
        },
    )
    .unwrap();
    let mut legacy = Control::new(status);
    let host = format!("127.0.0.1:{port}");
    let origin = format!("http://{host}");
    let policy = Policy {
        host: host.as_bytes(),
        origin: origin.as_bytes(),
        bearer_token: b"synthetic-token-not-a-real-secret",
    };
    let request = format!(
        "POST /v2/scan HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer synthetic-token-not-a-real-secret\r\nOrigin: {origin}\r\nConnection: keep-alive\r\nX-PS5-BT-Control: 1\r\nX-PS5-BT-Generation: 9:0\r\n\r\n"
    );
    peer.write_all(request.as_bytes()).unwrap();
    accepted(&mut listener, |client| {
        let (mut rx, mut tx) = ([0; 1024], [0; 16384]);
        let start = Instant::now();
        let mut output = Vec::new();
        let mut eof = false;
        {
            let mut pump = ConnectionPump::new(client, 0, 2_000_000, &mut rx, &mut tx).unwrap();
            for _ in 0..100000 {
                let now = start.elapsed().as_micros() as u64;
                assert!(now < 2_000_000, "bounded loopback timeout");
                let progress = pump.step(now, policy, &mut legacy, &mut manager).unwrap();
                let mut bytes = [0; 512];
                match peer.read(&mut bytes) {
                    Ok(0) => {
                        eof = true;
                    }
                    Ok(n) => output.extend(&bytes[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => panic!("peer read {e}"),
                };
                if progress == Progress::Closed && eof {
                    break;
                }
                std::thread::yield_now();
            }
            assert_eq!(pump.outcome(), Some(Ok(())));
        }
        assert!(eof);
        assert!(output.starts_with(b"HTTP/1.1 202 Accepted"));
        assert_eq!(manager.pending().unwrap().ticket.action, Action::Scan);
    });
    listener.close().unwrap();
}
#[test]
fn invalid_address_port_or_storage_never_becomes_a_listener() {
    for config in [
        BindConfig {
            address: [0, 0, 0, 0],
            port: 1,
        },
        BindConfig {
            address: [224, 0, 0, 1],
            port: 1,
        },
        BindConfig {
            address: [192, 168, 1, 1],
            port: 0,
        },
    ] {
        assert!(Listener::bind(&mut ListenerStorage::<128>::new(), config).is_err());
    }
    assert!(Listener::bind(&mut ListenerStorage::<1>::new(), config()).is_err());
}

#[test]
fn idle_client_deadline_closes_real_socket_and_allows_next_accept() {
    let mut storage = ListenerStorage::<128>::new();
    let mut listener = Listener::bind(&mut storage, config()).unwrap();
    let mut peer = connect(listener.local_port());
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let status = Status {
        transport: Capability::Unavailable,
        capture: Capability::Unavailable,
        codec: Capability::Unavailable,
        playback: Capability::Unavailable,
        link: LinkState::Disconnected,
        stream: StreamState::Stopped,
        last_error: None,
        revision: 0,
    };
    let mut legacy = Control::new(status);
    let mut manager = Manager::<1>::new(
        9,
        Snapshot {
            status,
            owner_phase: OwnerPhase::Idle,
            cleanup_required: false,
            scanning: false,
            selected: None,
            approval: None,
            available: Actions::none(),
        },
    )
    .unwrap();
    accepted(&mut listener, |client| {
        let (mut rx, mut tx) = ([0; 1024], [0; 16384]);
        let mut pump = ConnectionPump::new(client, 0, 100, &mut rx, &mut tx).unwrap();
        let failure = pump
            .step(
                100,
                Policy {
                    host: b"127.0.0.1",
                    origin: b"http://127.0.0.1",
                    bearer_token: b"synthetic-token",
                },
                &mut legacy,
                &mut manager,
            )
            .unwrap_err();
        assert_eq!(failure.cause, bt_control::pump::Cause::Timeout);
        assert_eq!(failure.cleanup, None);
    });
    assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
    assert!(manager.pending().is_none());
    let _next = connect(listener.local_port());
    accepted(&mut listener, |mut client| client.close().unwrap());
    listener.close().unwrap();
}
