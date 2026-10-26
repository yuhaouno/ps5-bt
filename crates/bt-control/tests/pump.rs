// SPDX-License-Identifier: GPL-3.0-or-later
use bt_control::{http::Policy, management::*, pump::*, *};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
const POLICY: Policy<'_> = Policy {
    host: b"console",
    origin: b"http://console",
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
#[derive(Default)]
struct Script {
    input: VecDeque<Result<Vec<u8>, IoError>>,
    output: Vec<u8>,
    write_limit: usize,
    write_errors: VecDeque<IoError>,
    closes: usize,
    close_error: Option<i32>,
    read_calls: usize,
    write_calls: usize,
    bad_read: bool,
    bad_write: bool,
}
#[derive(Clone)]
struct Fake(Rc<RefCell<Script>>);
impl SocketIo for Fake {
    fn read(&mut self, out: &mut [u8]) -> Result<usize, IoError> {
        let mut s = self.0.borrow_mut();
        s.read_calls += 1;
        if s.bad_read {
            return Ok(out.len() + 1);
        }
        match s.input.pop_front().unwrap_or(Err(IoError::WouldBlock)) {
            Ok(mut data) => {
                let len = data.len().min(out.len());
                out[..len].copy_from_slice(&data[..len]);
                if len < data.len() {
                    data.drain(..len);
                    s.input.push_front(Ok(data));
                }
                Ok(len)
            }
            Err(e) => Err(e),
        }
    }
    fn write(&mut self, data: &[u8]) -> Result<usize, IoError> {
        let mut s = self.0.borrow_mut();
        s.write_calls += 1;
        if s.bad_write {
            return Ok(data.len() + 1);
        }
        if let Some(e) = s.write_errors.pop_front() {
            return Err(e);
        }
        let len = data.len().min(s.write_limit);
        s.output.extend(&data[..len]);
        Ok(len)
    }
    fn close(&mut self) -> Result<(), i32> {
        let mut s = self.0.borrow_mut();
        s.closes += 1;
        s.close_error.map_or(Ok(()), Err)
    }
}
fn fake(input: Vec<Result<Vec<u8>, IoError>>) -> (Fake, Rc<RefCell<Script>>) {
    let shared = Rc::new(RefCell::new(Script {
        input: input.into(),
        write_limit: 17,
        ..Script::default()
    }));
    (Fake(shared.clone()), shared)
}
fn request() -> Vec<u8> {
    b"POST /v2/scan HTTP/1.1\r\nHost: console\r\nAuthorization: Bearer synthetic-secret-token\r\nOrigin: http://console\r\nX-PS5-BT-Control: 1\r\nX-PS5-BT-Generation: 7:0\r\n\r\n".to_vec()
}
#[test]
fn fragmented_request_and_partial_writes_execute_once_under_backpressure() {
    let req = request();
    let (io, shared) = fake(vec![
        Ok(req[..31].to_vec()),
        Err(IoError::WouldBlock),
        Ok(req[31..].to_vec()),
    ]);
    shared
        .borrow_mut()
        .write_errors
        .push_back(IoError::WouldBlock);
    let (mut rx, mut tx) = ([0; 1024], [0; 16384]);
    let mut m = manager();
    let mut legacy = Control::new(status());
    {
        let mut pump = ConnectionPump::new(io, 10, 1000, &mut rx, &mut tx).unwrap();
        assert_eq!(
            pump.step(10, POLICY, &mut legacy, &mut m),
            Ok(Progress::Reading)
        );
        assert!(m.pending().is_none());
        assert_eq!(
            pump.step(11, POLICY, &mut legacy, &mut m),
            Ok(Progress::Reading)
        );
        assert_eq!(shared.borrow().read_calls, 2);
        assert!(matches!(
            pump.step(12, POLICY, &mut legacy, &mut m),
            Ok(Progress::Writing { .. })
        ));
        let ticket = m.pending().unwrap().ticket;
        for now in 13..200 {
            if pump.step(now, POLICY, &mut legacy, &mut m) == Ok(Progress::Closed) {
                break;
            }
        }
        assert!(pump.is_closed());
        assert_eq!(pump.outcome(), Some(Ok(())));
        assert_eq!(m.pending().unwrap().ticket, ticket);
        assert_eq!(
            pump.step(9999, POLICY, &mut legacy, &mut m),
            Ok(Progress::Closed)
        );
    }
    let s = shared.borrow();
    assert_eq!(s.closes, 1);
    assert_eq!(s.read_calls, 3);
    assert!(s.output.starts_with(b"HTTP/1.1 202 Accepted"));
    assert!(s.write_calls > 2);
    assert_eq!(rx, [0; 1024]);
    assert_eq!(tx, [0; 16384]);
}
#[test]
fn fixed_deadline_does_not_extend_with_slow_headers_or_wouldblock() {
    let (io, shared) = fake(vec![Ok(b"GET ".to_vec()), Err(IoError::WouldBlock)]);
    let (mut rx, mut tx) = ([0; 1024], [0; 2048]);
    let mut m = manager();
    let mut legacy = Control::new(status());
    let mut pump = ConnectionPump::new(io, 10, 100, &mut rx, &mut tx).unwrap();
    pump.step(20, POLICY, &mut legacy, &mut m).unwrap();
    pump.step(109, POLICY, &mut legacy, &mut m).unwrap();
    let error = pump.step(110, POLICY, &mut legacy, &mut m).unwrap_err();
    assert_eq!(error.cause, Cause::Timeout);
    assert_eq!(shared.borrow().closes, 1);
    assert_eq!(pump.cancel(), Err(error));
    assert_eq!(shared.borrow().closes, 1);
}
#[test]
fn clock_eof_io_failure_and_dirty_counts_close_once_with_original_cause() {
    for case in 0..5 {
        let (io, shared) = fake(vec![match case {
            1 => Ok(vec![]),
            2 => Err(IoError::Failed(-8)),
            _ => Err(IoError::WouldBlock),
        }]);
        if case == 3 {
            shared.borrow_mut().bad_read = true
        }
        shared.borrow_mut().close_error = Some(-19);
        let (mut rx, mut tx) = ([0; 1024], [0; 2048]);
        let mut m = manager();
        let mut legacy = Control::new(status());
        {
            let mut p = ConnectionPump::new(io, 10, 100, &mut rx, &mut tx).unwrap();
            let e = if case == 4 {
                p.cancel().unwrap_err()
            } else {
                p.step(if case == 0 { 9 } else { 10 }, POLICY, &mut legacy, &mut m)
                    .unwrap_err()
            };
            assert_eq!(
                e.cause,
                match case {
                    0 => Cause::ClockRegression,
                    1 => Cause::Eof,
                    2 => Cause::Read(-8),
                    3 => Cause::InvalidCount,
                    _ => Cause::Cancelled,
                }
            );
            assert_eq!(e.cleanup, Some(-19));
            assert_eq!(p.outcome(), Some(Err(e)));
        }
        assert_eq!(shared.borrow().closes, 1);
    }
}
#[test]
fn write_failure_zero_bad_count_and_reply_timeout_do_not_undo_queued_work() {
    for case in 0..4 {
        let (io, shared) = fake(vec![Ok(request())]);
        match case {
            0 => shared
                .borrow_mut()
                .write_errors
                .push_back(IoError::Failed(-9)),
            1 => shared.borrow_mut().write_limit = 0,
            2 => shared.borrow_mut().bad_write = true,
            _ => {}
        };
        let (mut rx, mut tx) = ([0; 1024], [0; 2048]);
        let mut m = manager();
        let mut legacy = Control::new(status());
        {
            let mut p = ConnectionPump::new(io, 10, 100, &mut rx, &mut tx).unwrap();
            p.step(10, POLICY, &mut legacy, &mut m).unwrap();
            let owned = m.pending().unwrap();
            let e = p
                .step(
                    if case == 3 { 110 } else { 10 },
                    POLICY,
                    &mut legacy,
                    &mut m,
                )
                .unwrap_err();
            assert_eq!(
                e.cause,
                match case {
                    0 => Cause::Write(-9),
                    1 => Cause::WriteZero,
                    2 => Cause::InvalidCount,
                    _ => Cause::Timeout,
                }
            );
            assert_eq!(m.pending(), Some(owned));
        }
        assert_eq!(shared.borrow().closes, 1);
    }
}
#[test]
fn initialization_and_drop_own_exactly_one_close_attempt() {
    let (io, shared) = fake(vec![]);
    assert!(ConnectionPump::new(io, 0, 0, &mut [0; 1024], &mut [0; 2048]).is_err());
    assert_eq!(shared.borrow().closes, 1);
    let (io, shared) = fake(vec![]);
    assert!(ConnectionPump::new(io, u64::MAX, 1, &mut [0; 1024], &mut [0; 2048]).is_err());
    assert_eq!(shared.borrow().closes, 1);
    let (io, shared) = fake(vec![]);
    {
        let (mut rx, mut tx) = ([0; 1024], [0; 2048]);
        let _p = ConnectionPump::new(io, 0, 100, &mut rx, &mut tx).unwrap();
    }
    assert_eq!(shared.borrow().closes, 1);
}
#[test]
fn pipelined_request_gets_one_error_response_and_never_submits() {
    let mut data = request();
    data.extend(request());
    let (io, shared) = fake(vec![Ok(data)]);
    let (mut rx, mut tx) = ([0; 1024], [0; 2048]);
    let mut m = manager();
    let mut legacy = Control::new(status());
    let mut p = ConnectionPump::new(io, 0, 1000, &mut rx, &mut tx).unwrap();
    for now in 0..100 {
        if p.step(now, POLICY, &mut legacy, &mut m) == Ok(Progress::Closed) {
            break;
        }
    }
    assert!(m.pending().is_none());
    assert!(shared.borrow().output.starts_with(b"HTTP/1.1 400"));
    assert_eq!(shared.borrow().closes, 1);
}

#[test]
fn successful_response_with_failed_close_reports_cleanup_uncertainty_once() {
    let (io, shared) = fake(vec![Ok(request())]);
    shared.borrow_mut().write_limit = usize::MAX;
    shared.borrow_mut().close_error = Some(-37);
    let (mut rx, mut tx) = ([0; 1024], [0; 2048]);
    let mut m = manager();
    let mut legacy = Control::new(status());
    {
        let mut p = ConnectionPump::new(io, 0, 100, &mut rx, &mut tx).unwrap();
        p.step(0, POLICY, &mut legacy, &mut m).unwrap();
        let error = p.step(1, POLICY, &mut legacy, &mut m).unwrap_err();
        assert_eq!(
            error,
            Failure {
                cause: Cause::Close(-37),
                cleanup: Some(-37)
            }
        );
        assert_eq!(p.cancel(), Err(error));
        assert!(m.pending().is_some());
    }
    assert_eq!(shared.borrow().closes, 1);
}
