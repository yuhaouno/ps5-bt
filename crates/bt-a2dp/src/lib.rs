// SPDX-License-Identifier: GPL-3.0-or-later
//! Allocation-free A2DP SBC-over-RTP packetization. No transport or session.
#![no_std]
#![forbid(unsafe_code)]

use bt_core::SbcParameters;

pub const RTP_HEADER_BYTES: usize = 12;
pub const SBC_MEDIA_HEADER_BYTES: usize = 1;
pub const PACKET_OVERHEAD_BYTES: usize = RTP_HEADER_BYTES + SBC_MEDIA_HEADER_BYTES;
pub const MAX_SBC_FRAMES_PER_PACKET: usize = 15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketError {
    InvalidPayloadType,
    InvalidSsrc,
    InvalidMtu,
    InvalidFrameCount,
    InvalidFrame,
    MtuExceeded,
    BufferTooSmall,
    Overflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PacketReport {
    pub bytes: usize,
    pub payload_bytes: usize,
    pub sequence: u16,
    pub timestamp: u32,
    pub frames: u8,
    pub marker: bool,
}

/// One fixed-configuration SBC RTP stream.
///
/// Sequence and timestamp are committed only after a complete packet is copied
/// into caller storage. The RTP timestamp counts PCM sample frames, not bytes or
/// channel samples. This type never fragments an SBC frame or writes a partial
/// packet. The caller creates a new instance after codec/MTU reconfiguration.
pub struct SbcRtpPacketizer {
    payload_type: u8,
    ssrc: u32,
    mtu: usize,
    frame_bytes: usize,
    samples_per_frame: u32,
    next_sequence: u16,
    next_timestamp: u32,
    marker_pending: bool,
}

impl SbcRtpPacketizer {
    pub fn new(
        parameters: SbcParameters,
        mtu: u16,
        payload_type: u8,
        ssrc: u32,
        first_sequence: u16,
        first_timestamp: u32,
    ) -> Result<Self, PacketError> {
        if !(96..=127).contains(&payload_type) {
            return Err(PacketError::InvalidPayloadType);
        }
        if ssrc == 0 {
            return Err(PacketError::InvalidSsrc);
        }
        let frame_bytes = parameters.frame_length_bytes();
        let samples_per_frame =
            u32::try_from(parameters.pcm_frames()).map_err(|_| PacketError::Overflow)?;
        let mtu = usize::from(mtu);
        let minimum = PACKET_OVERHEAD_BYTES
            .checked_add(frame_bytes)
            .ok_or(PacketError::Overflow)?;
        if frame_bytes == 0 || samples_per_frame == 0 || mtu < minimum {
            return Err(PacketError::InvalidMtu);
        }
        Ok(Self {
            payload_type,
            ssrc,
            mtu,
            frame_bytes,
            samples_per_frame,
            next_sequence: first_sequence,
            next_timestamp: first_timestamp,
            marker_pending: false,
        })
    }

    pub const fn mtu(&self) -> usize {
        self.mtu
    }

    pub const fn frame_bytes(&self) -> usize {
        self.frame_bytes
    }

    pub const fn samples_per_frame(&self) -> u32 {
        self.samples_per_frame
    }

    pub const fn next_sequence(&self) -> u16 {
        self.next_sequence
    }

    pub const fn next_timestamp(&self) -> u32 {
        self.next_timestamp
    }

    pub const fn marker_pending(&self) -> bool {
        self.marker_pending
    }

    /// Mark the next successfully emitted packet. Failed attempts retain it.
    /// The caller decides when its A2DP/RTP session semantics require a marker;
    /// the packetizer does not infer a talkspurt from PCM contents or wall time.
    pub fn mark_next_packet(&mut self) {
        self.marker_pending = true;
    }

    /// Packetize one to fifteen complete, fixed-configuration SBC frames.
    ///
    /// Each frame must have the negotiated length and SBC sync byte. Full SBC
    /// syntax/CRC validation remains the encoder/decoder boundary's job.
    /// Any error leaves output and all packetizer state unchanged.
    pub fn packetize(
        &mut self,
        frames: &[&[u8]],
        output: &mut [u8],
    ) -> Result<PacketReport, PacketError> {
        if frames.is_empty() || frames.len() > MAX_SBC_FRAMES_PER_PACKET {
            return Err(PacketError::InvalidFrameCount);
        }
        for frame in frames {
            if frame.len() != self.frame_bytes || frame.first() != Some(&0x9c) {
                return Err(PacketError::InvalidFrame);
            }
        }
        let payload_bytes = self
            .frame_bytes
            .checked_mul(frames.len())
            .ok_or(PacketError::Overflow)?;
        let packet_bytes = PACKET_OVERHEAD_BYTES
            .checked_add(payload_bytes)
            .ok_or(PacketError::Overflow)?;
        if packet_bytes > self.mtu {
            return Err(PacketError::MtuExceeded);
        }
        if output.len() < packet_bytes {
            return Err(PacketError::BufferTooSmall);
        }
        let sequence = self.next_sequence;
        let timestamp = self.next_timestamp;
        let marker = self.marker_pending;

        output[0] = 0x80;
        output[1] = self.payload_type | if marker { 0x80 } else { 0 };
        output[2..4].copy_from_slice(&sequence.to_be_bytes());
        output[4..8].copy_from_slice(&timestamp.to_be_bytes());
        output[8..12].copy_from_slice(&self.ssrc.to_be_bytes());
        // F/S/L/reserved are zero: only complete, non-fragmented frames.
        output[12] = frames.len() as u8;
        let mut offset = PACKET_OVERHEAD_BYTES;
        for frame in frames {
            let end = offset + frame.len();
            output[offset..end].copy_from_slice(frame);
            offset = end;
        }

        self.next_sequence = sequence.wrapping_add(1);
        self.next_timestamp =
            timestamp.wrapping_add(self.samples_per_frame.wrapping_mul(frames.len() as u32));
        self.marker_pending = false;
        Ok(PacketReport {
            bytes: packet_bytes,
            payload_bytes,
            sequence,
            timestamp,
            frames: frames.len() as u8,
            marker,
        })
    }
}
