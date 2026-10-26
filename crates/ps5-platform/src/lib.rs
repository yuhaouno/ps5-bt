// SPDX-License-Identifier: GPL-3.0-or-later
//! Safe wrapper. The linked C backend must satisfy docs/ABI.md.
#![no_std]

#[cfg(feature = "external-clock-wait")]
pub mod clock_wait;

use bt_core::{BluetoothTransport, Error, PacketKind, Received};
use core::{marker::PhantomData, num::NonZeroU32};
use ps5_sys as sys;

fn status(code: i32) -> Result<(), Error> {
    match code {
        sys::OK => Ok(()),
        sys::UNSUPPORTED => Err(Error::Unsupported),
        sys::INVALID => Err(Error::Invalid),
        sys::BUSY => Err(Error::Busy),
        sys::BUFFER_TOO_SMALL => Err(Error::BufferTooSmall),
        sys::IO => Err(Error::Io),
        sys::DISCONNECTED => Err(Error::Disconnected),
        sys::INTEGRITY_LOST => Err(Error::IntegrityLost),
        sys::TIMEOUT => Err(Error::Timeout),
        other => Err(Error::Backend(other)),
    }
}

/// Synchronous logging; large messages are chunked without retaining pointers.
pub fn log(message: &[u8]) {
    for chunk in message.chunks(u32::MAX as usize) {
        // SAFETY: chunk is readable for len bytes until this synchronous call returns.
        unsafe { sys::ps5_platform_log(chunk.as_ptr(), chunk.len() as u32) };
    }
}

/// Terminate through the C runtime; no unwind or return across FFI.
pub fn abort() -> ! {
    // SAFETY: the linked platform abort function has no preconditions and never returns.
    unsafe { sys::ps5_platform_abort() }
}

/// Owns a session token. Raw-pointer marker prevents both Send and Sync.
pub struct Transport {
    token: Option<NonZeroU32>,
    _single_thread: PhantomData<*mut ()>,
}

impl Transport {
    pub fn open() -> Result<Self, Error> {
        // SAFETY: pure, argument-free ABI query.
        if unsafe { sys::ps5_bt_abi_version() } != sys::ABI_VERSION {
            return Err(Error::AbiMismatch);
        }
        let mut token = 0;
        // SAFETY: output is a live u32; init retains no pointer and cleans up on failure.
        status(unsafe { sys::ps5_bt_init(&mut token) })?;
        let token = NonZeroU32::new(token).ok_or(Error::IntegrityLost)?;
        Ok(Self {
            token: Some(token),
            _single_thread: PhantomData,
        })
    }

    /// Fallible cleanup. The C ABI revokes the token even on cleanup failure.
    pub fn close(mut self) -> Result<(), Error> {
        self.shutdown()
    }

    fn token(&self) -> u32 {
        self.token.expect("live Transport has a token").get()
    }

    fn shutdown(&mut self) -> Result<(), Error> {
        if let Some(token) = self.token.take() {
            // SAFETY: token is owned here and consumed once; C retains only C-owned memory.
            status(unsafe { sys::ps5_bt_shutdown(token.get()) })
        } else {
            Ok(())
        }
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        if self.shutdown().is_err() {
            log(b"event=shutdown_failed context=drop\n");
        }
    }
}

impl BluetoothTransport for Transport {
    fn send_command(&mut self, packet: &[u8]) -> Result<(), Error> {
        if packet.len() < 3 || packet.len() != usize::from(packet[2]) + 3 {
            return Err(Error::Invalid);
        }
        // SAFETY: complete checked command, live token, readable buffer retained only in call.
        status(unsafe {
            sys::ps5_bt_send_hci_command(self.token(), packet.as_ptr(), packet.len() as u32)
        })
    }

    fn send_acl(&mut self, packet: &[u8]) -> Result<(), Error> {
        if packet.len() < 4
            || packet.len() != usize::from(u16::from_le_bytes([packet[2], packet[3]])) + 4
        {
            return Err(Error::Invalid);
        }
        // SAFETY: complete checked ACL frame, live token, no pointer retained beyond the call.
        status(unsafe { sys::ps5_bt_send_acl(self.token(), packet.as_ptr(), packet.len() as u32) })
    }

    fn receive<'a>(
        &mut self,
        buffer: &'a mut [u8],
        timeout_ms: u32,
    ) -> Result<Option<Received<'a>>, Error> {
        let capacity = u32::try_from(buffer.len()).map_err(|_| Error::BufferTooSmall)?;
        let (mut len, mut kind) = (0, 0);
        // SAFETY: exclusive writable buffer with exact capacity and live output u32s.
        // The backend must obey capacity and lifetime rules even when it fails.
        let code = unsafe {
            sys::ps5_bt_receive(
                self.token(),
                buffer.as_mut_ptr(),
                capacity,
                &mut len,
                &mut kind,
                timeout_ms,
            )
        };
        if code == sys::TIMEOUT {
            return if len == 0 && kind == 0 {
                Ok(None)
            } else {
                Err(Error::IntegrityLost)
            };
        }
        status(code)?;
        let bytes = buffer.get(..len as usize).ok_or(Error::IntegrityLost)?;
        let kind = match kind {
            sys::EVENT if bytes.len() >= 2 && bytes.len() == usize::from(bytes[1]) + 2 => {
                PacketKind::Event
            }
            sys::ACL
                if bytes.len() >= 4
                    && bytes.len() == usize::from(u16::from_le_bytes([bytes[2], bytes[3]])) + 4 =>
            {
                PacketKind::Acl
            }
            _ => return Err(Error::IntegrityLost),
        };
        Ok(Some(Received { kind, bytes }))
    }
}
