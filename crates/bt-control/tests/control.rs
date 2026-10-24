// SPDX-License-Identifier: GPL-3.0-or-later

use bt_control::{
    Capability, Command, CommandOutcome, Control, FinishError, LinkState, Status, StatusWriteError,
    StreamState, SubmitError,
};

fn status() -> Status {
    Status {
        transport: Capability::Ready,
        capture: Capability::Unavailable,
        codec: Capability::Ready,
        playback: Capability::Ready,
        link: LinkState::Disconnected,
        stream: StreamState::Stopped,
        last_error: None,
        revision: 7,
    }
}

#[test]
fn commands_are_serialized_and_finished_exactly_once() {
    let mut control = Control::new(status());
    let connect = control.submit(Command::Connect).unwrap();
    assert_eq!(control.status().link, LinkState::Connecting);
    assert_eq!(control.submit(Command::Stop), Err(SubmitError::Busy));
    assert_eq!(control.pending(), Some(connect));
    control.finish(connect, CommandOutcome::Success).unwrap();
    assert_eq!(control.status().link, LinkState::Connected);
    assert_eq!(
        control.finish(connect, CommandOutcome::Success),
        Err(FinishError::NoPendingCommand)
    );

    let start = control.submit(Command::Start).unwrap();
    assert_eq!(control.status().stream, StreamState::Starting);
    let stale = bt_control::PendingCommand {
        id: start.id.wrapping_add(1),
        command: Command::Start,
    };
    assert_eq!(
        control.finish(stale, CommandOutcome::Success),
        Err(FinishError::StaleCommand)
    );
    assert_eq!(control.pending(), Some(start));
    control.finish(start, CommandOutcome::Success).unwrap();

    let stop = control.submit(Command::Stop).unwrap();
    control.finish(stop, CommandOutcome::Cancelled).unwrap();
    assert_eq!(control.status().stream, StreamState::Streaming);

    let stop = control.submit(Command::Stop).unwrap();
    control
        .finish(stop, CommandOutcome::Failed(-2147483648))
        .unwrap();
    assert_eq!(control.status().stream, StreamState::Streaming);
    assert_eq!(control.status().last_error, Some(i32::MIN));
}

#[test]
fn unavailable_and_invalid_transitions_do_not_mutate_state() {
    let mut initial = status();
    initial.transport = Capability::Unsupported;
    initial.playback = Capability::Unavailable;
    let mut control = Control::new(initial);
    assert_eq!(
        control.submit(Command::Connect),
        Err(SubmitError::Unavailable)
    );
    assert_eq!(
        control.submit(Command::Start),
        Err(SubmitError::Unavailable)
    );
    assert_eq!(
        control.submit(Command::Stop),
        Err(SubmitError::InvalidState)
    );
    assert_eq!(control.status(), initial);
    assert_eq!(control.pending(), None);
}

#[test]
fn status_json_is_exact_bounded_and_contains_no_identity_data() {
    let mut control = Control::new(status());
    let pending = control.submit(Command::Connect).unwrap();
    control
        .finish(pending, CommandOutcome::Failed(i32::MIN))
        .unwrap();
    let mut output = [0xa5; 256];
    let length = control.write_status(&mut output).unwrap();
    assert_eq!(
        &output[..length],
        b"{\"transport\":\"ready\",\"capture\":\"unavailable\",\"codec\":\"ready\",\"playback\":\"ready\",\"link\":\"disconnected\",\"stream\":\"stopped\",\"pending\":null,\"last_error\":-2147483648,\"revision\":9}"
    );
    assert_eq!(output[length], 0xa5);

    let mut short = [0xa5; 32];
    assert_eq!(
        control.write_status(&mut short),
        Err(StatusWriteError::BufferTooSmall)
    );
    assert_eq!(short, [0xa5; 32]);
}

#[test]
fn observation_cannot_replace_an_owned_command() {
    let mut control = Control::new(status());
    let pending = control.submit(Command::Connect).unwrap();
    assert_eq!(control.observe(status()), Err(SubmitError::Busy));
    control.finish(pending, CommandOutcome::Cancelled).unwrap();
    let mut observed = status();
    observed.capture = Capability::Ready;
    control.observe(observed).unwrap();
    assert_eq!(control.status().capture, Capability::Ready);
    assert_eq!(control.status().revision, 10);
}
