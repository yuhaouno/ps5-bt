// SPDX-License-Identifier: GPL-3.0-or-later
//! C/Rust diagnostic seam. No live controller backend is shipped.
//! This one-response exercise is not a credit-aware live HCI dispatcher.
#![cfg_attr(target_os = "none", no_std)]

use bt_core::{BluetoothTransport, Error, PacketKind};
use ps5_platform::{Transport, log};

/// Fixed-width numeric logging keeps diagnostics allocation-free on the target.
fn field(name: &[u8], value: u32) {
    let mut encoded = [0_u8; 9];
    for (i, byte) in encoded[..8].iter_mut().enumerate() {
        *byte = b"0123456789abcdef"[((value >> (28 - i * 4)) & 15) as usize];
    }
    encoded[8] = b'\n';
    log(name);
    log(&encoded);
}

fn report_error(error: Error) {
    let code = match error {
        Error::Unsupported => -1,
        Error::Invalid => -2,
        Error::Busy => -3,
        Error::BufferTooSmall => -4,
        Error::Io => -5,
        Error::Disconnected => -6,
        Error::IntegrityLost => -7,
        Error::Timeout => -8,
        Error::AbiMismatch => {
            log(b"error=abi_mismatch\n");
            return;
        }
        Error::Backend(code) => code,
    };
    // Retain the exact signed i32 bit pattern, including unknown backend values.
    field(b"backend_status_bits=0x", code as u32);
}

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    log(b"event=rust_panic\n");
    ps5_platform::abort()
}

fn diagnostic(transport: &mut Transport) -> Result<(), Error> {
    transport.send_command(&bt_hci::READ_LOCAL_VERSION)?;
    let mut buffer = [0; 257];
    let packet = transport
        .receive(&mut buffer, 1000)?
        .ok_or(Error::Timeout)?;
    if packet.kind != PacketKind::Event {
        return Err(Error::IntegrityLost);
    }
    let version = bt_hci::LocalVersion::parse(packet.bytes).map_err(|error| {
        match error {
            bt_hci::ParseError::ControllerStatus(status) => field(b"hci_status=0x", status.into()),
            bt_hci::ParseError::Length => log(b"parse_error=length\n"),
            bt_hci::ParseError::UnexpectedEvent => log(b"parse_error=unexpected_event\n"),
            bt_hci::ParseError::UnexpectedOpcode => log(b"parse_error=unexpected_opcode\n"),
            bt_hci::ParseError::InvalidHandle => log(b"parse_error=invalid_handle\n"),
            bt_hci::ParseError::InvalidFlags => log(b"parse_error=invalid_flags\n"),
        }
        Error::IntegrityLost
    })?;
    field(b"hci_version=0x", version.hci_version.into());
    field(b"hci_revision=0x", version.hci_revision.into());
    field(b"lmp_version=0x", version.lmp_version.into());
    field(b"manufacturer=0x", version.manufacturer.into());
    field(b"lmp_subversion=0x", version.lmp_subversion.into());
    Ok(())
}

/// Called by our C main. Returns 0 only for the synthetic successful probe,
/// 2 for unsupported hardware adapter, and 1 for an error. Never enables audio.
// SAFETY: this unique symbol has the primitive-only signature in ps5_bt.h.
#[cfg_attr(not(test), unsafe(no_mangle))]
pub extern "C" fn ps5_rust_probe() -> i32 {
    log(b"event=rust_entry abi=1 phase=bootstrap\n");
    let mut transport = match Transport::open() {
        Ok(transport) => transport,
        Err(Error::Unsupported) => {
            log(b"event=adapter_unavailable status=unsupported\n");
            return 2;
        }
        Err(error) => {
            log(b"event=adapter_open_failed\n");
            report_error(error);
            return 1;
        }
    };
    let result = diagnostic(&mut transport);
    let cleanup = transport.close();
    if let Err(error) = result {
        log(b"event=diagnostic_failed\n");
        report_error(error);
    }
    if let Err(error) = cleanup {
        log(b"event=shutdown_failed\n");
        report_error(error);
    }
    if result.is_err() || cleanup.is_err() {
        log(b"event=probe_failed\n");
        return 1;
    }
    log(b"event=probe_complete\n");
    0
}
