// SPDX-License-Identifier: GPL-3.0-or-later

use bt_a2dp::{PACKET_OVERHEAD_BYTES, PacketError, SbcRtpPacketizer};
use bt_core::{
    SbcAllocation, SbcBlocks, SbcChannelMode, SbcParameters, SbcSampleRate, SbcSubbands,
};

fn parameters() -> SbcParameters {
    SbcParameters::new(
        SbcSampleRate::Hz48000,
        SbcChannelMode::JointStereo,
        SbcBlocks::Sixteen,
        SbcSubbands::Eight,
        SbcAllocation::Loudness,
        32,
    )
    .unwrap()
}

fn frame(fill: u8) -> [u8; 77] {
    let mut frame = [fill; 77];
    frame[0] = 0x9c;
    frame
}

#[test]
fn writes_exact_rtp_and_a2dp_sbc_headers() {
    let encoded = frame(0x44);
    let mut output = [0xa5; 100];
    let mut packetizer =
        SbcRtpPacketizer::new(parameters(), 100, 96, 0x1122_3344, 0x5566, 0x7788_99aa).unwrap();
    packetizer.mark_next_packet();
    let report = packetizer.packetize(&[&encoded], &mut output).unwrap();

    assert_eq!((report.bytes, report.payload_bytes), (90, 77));
    assert_eq!((report.sequence, report.timestamp), (0x5566, 0x7788_99aa));
    assert_eq!((report.frames, report.marker), (1, true));
    assert_eq!(
        &output[..PACKET_OVERHEAD_BYTES],
        &[
            0x80, 0xe0, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0x11, 0x22, 0x33, 0x44, 1
        ]
    );
    assert_eq!(&output[PACKET_OVERHEAD_BYTES..90], &encoded);
    assert_eq!(&output[90..], &[0xa5; 10]);
    assert_eq!(packetizer.next_sequence(), 0x5567);
    assert_eq!(packetizer.next_timestamp(), 0x7788_9a2a);
    assert!(!packetizer.marker_pending());
}

#[test]
fn multiple_frames_use_sample_timestamps_and_count_header() {
    let first = frame(1);
    let second = frame(2);
    let third = frame(3);
    let mut output = [0; 244];
    let mut packetizer = SbcRtpPacketizer::new(parameters(), 244, 127, 1, 9, 1000).unwrap();
    let report = packetizer
        .packetize(&[&first, &second, &third], &mut output)
        .unwrap();
    assert_eq!(report.bytes, 244);
    assert_eq!(output[1], 127);
    assert_eq!(output[12], 3);
    assert_eq!(&output[13..90], &first);
    assert_eq!(&output[90..167], &second);
    assert_eq!(&output[167..244], &third);
    assert_eq!(packetizer.next_timestamp(), 1000 + 3 * 128);
}

#[test]
fn backpressure_and_mtu_failures_are_atomic_and_retryable() {
    let encoded = frame(0x33);
    let mut packetizer = SbcRtpPacketizer::new(parameters(), 100, 96, 1, 4, 800).unwrap();
    packetizer.mark_next_packet();
    let mut short = [0xa5; 89];
    assert_eq!(
        packetizer.packetize(&[&encoded], &mut short),
        Err(PacketError::BufferTooSmall)
    );
    assert_eq!(short, [0xa5; 89]);
    assert_eq!(
        (packetizer.next_sequence(), packetizer.next_timestamp()),
        (4, 800)
    );
    assert!(packetizer.marker_pending());

    let second = frame(0x55);
    let mut output = [0xa5; 200];
    assert_eq!(
        packetizer.packetize(&[&encoded, &second], &mut output),
        Err(PacketError::MtuExceeded)
    );
    assert_eq!(output, [0xa5; 200]);
    assert_eq!(
        (packetizer.next_sequence(), packetizer.next_timestamp()),
        (4, 800)
    );
    assert!(packetizer.marker_pending());

    let report = packetizer.packetize(&[&encoded], &mut output).unwrap();
    assert_eq!(
        (report.sequence, report.timestamp, report.marker),
        (4, 800, true)
    );
}

#[test]
fn frame_count_and_frame_boundaries_are_rejected_atomically() {
    let valid = frame(0x11);
    let mut output = [0xa5; 1400];
    let mut packetizer = SbcRtpPacketizer::new(parameters(), 1400, 96, 1, 0, 0).unwrap();
    assert_eq!(
        packetizer.packetize(&[], &mut output),
        Err(PacketError::InvalidFrameCount)
    );
    let sixteen = [&valid[..]; 16];
    assert_eq!(
        packetizer.packetize(&sixteen, &mut output),
        Err(PacketError::InvalidFrameCount)
    );
    assert_eq!(
        packetizer.packetize(&[&valid[..76]], &mut output),
        Err(PacketError::InvalidFrame)
    );
    let mut wrong_sync = valid;
    wrong_sync[0] = 0;
    assert_eq!(
        packetizer.packetize(&[&wrong_sync], &mut output),
        Err(PacketError::InvalidFrame)
    );
    assert_eq!(output, [0xa5; 1400]);
    assert_eq!(
        (packetizer.next_sequence(), packetizer.next_timestamp()),
        (0, 0)
    );
}

#[test]
fn sequence_and_timestamp_wrap_in_wire_units() {
    let encoded = frame(0x22);
    let mut output = [0; 90];
    let mut packetizer =
        SbcRtpPacketizer::new(parameters(), 90, 96, 1, u16::MAX, u32::MAX - 63).unwrap();
    let report = packetizer.packetize(&[&encoded], &mut output).unwrap();
    assert_eq!(
        (report.sequence, report.timestamp),
        (u16::MAX, u32::MAX - 63)
    );
    assert_eq!(
        (packetizer.next_sequence(), packetizer.next_timestamp()),
        (0, 64)
    );
}

#[test]
fn constructor_rejects_non_dynamic_payloads_zero_ssrc_and_short_mtu() {
    assert!(matches!(
        SbcRtpPacketizer::new(parameters(), 90, 95, 1, 0, 0),
        Err(PacketError::InvalidPayloadType)
    ));
    assert!(matches!(
        SbcRtpPacketizer::new(parameters(), 90, 96, 0, 0, 0),
        Err(PacketError::InvalidSsrc)
    ));
    assert!(matches!(
        SbcRtpPacketizer::new(parameters(), 89, 96, 1, 0, 0),
        Err(PacketError::InvalidMtu)
    ));
}
