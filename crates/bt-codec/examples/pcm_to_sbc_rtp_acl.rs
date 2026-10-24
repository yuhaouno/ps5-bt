// SPDX-License-Identifier: GPL-3.0-or-later
//! Host-only proof: real SBC enters one RTP/L2CAP/HCI ACL packet.

use bt_a2dp::SbcRtpPacketizer;
use bt_codec::{Encoder, EncoderStorage};
use bt_core::media::{PcmQueue, RenderOutcome, TimedStereoFrame};
use bt_core::{
    SbcAllocation, SbcBitpoolRange, SbcBlocks, SbcChannelMode, SbcParameters, SbcSampleRate,
    SbcSubbands,
};
use bt_hci::{AclPacket, BroadcastFlag, ConnectionHandle, PacketBoundary};
use bt_l2cap::BasicFrame;

fn main() {
    let mut queue_storage = [TimedStereoFrame::default(); 128];
    let mut queue = PcmQueue::new(&mut queue_storage, 0).unwrap();
    let source: [[i16; 2]; 128] = core::array::from_fn(|index| {
        let sample = ((index as i16 % 40) - 20) * 800;
        [sample, -sample]
    });
    queue.push(0, &source).unwrap();
    let mut rendered = [[0_i16; 2]; 128];
    assert!(matches!(
        queue.render(0, &mut rendered).unwrap(),
        RenderOutcome::Rendered(_)
    ));
    let mut interleaved = [0_i16; 256];
    for (destination, frame) in interleaved.chunks_exact_mut(2).zip(rendered) {
        destination.copy_from_slice(&frame);
    }

    let parameters = SbcParameters::new(
        SbcSampleRate::Hz48000,
        SbcChannelMode::JointStereo,
        SbcBlocks::Sixteen,
        SbcSubbands::Eight,
        SbcAllocation::Loudness,
        32,
    )
    .unwrap();
    let mut encoder_storage = EncoderStorage::<8192>::new();
    let mut encoder = Encoder::new(
        &mut encoder_storage,
        parameters,
        SbcBitpoolRange::new(2, 53).unwrap(),
    )
    .unwrap();
    let mut sbc = [0_u8; 119];
    let encoded = encoder.encode_frame(&interleaved, &mut sbc).unwrap();
    encoder.finish().unwrap();
    assert_eq!((encoded.pcm_frames, encoded.bytes), (128, 77));

    let mut rtp_storage = [0_u8; 100];
    let mut packetizer = SbcRtpPacketizer::new(parameters, 100, 96, 0x1122_3344, 0, 0).unwrap();
    packetizer.mark_next_packet();
    let packet = packetizer
        .packetize(&[&sbc[..encoded.bytes]], &mut rtp_storage)
        .unwrap();
    assert_eq!((packet.bytes, packet.frames, packet.timestamp), (90, 1, 0));

    let mut l2cap_storage = [0_u8; 104];
    let l2cap_bytes = BasicFrame::new(0x0041, &rtp_storage[..packet.bytes])
        .unwrap()
        .encode(&mut l2cap_storage)
        .unwrap();
    assert_eq!(l2cap_bytes, 94);

    let mut acl_storage = [0_u8; 108];
    let acl_bytes = AclPacket::new(
        ConnectionHandle::new(0x000b).unwrap(),
        PacketBoundary::StartNonFlushable,
        BroadcastFlag::PointToPoint,
        &l2cap_storage[..l2cap_bytes],
    )
    .unwrap()
    .encode(&mut acl_storage)
    .unwrap();
    assert_eq!(acl_bytes, 98);

    let decoded_acl = AclPacket::parse(&acl_storage[..acl_bytes]).unwrap();
    let decoded_l2cap = BasicFrame::parse(decoded_acl.payload()).unwrap();
    assert_eq!(decoded_l2cap.channel_id(), 0x0041);
    assert_eq!(&decoded_l2cap.payload()[13..], &sbc[..77]);
    assert_eq!(decoded_l2cap.payload()[0], 0x80);
    assert_eq!(decoded_l2cap.payload()[1], 0xe0);
    assert_eq!(decoded_l2cap.payload()[12], 1);

    println!(
        "host-only pipeline: 128 PCM frames -> 77 SBC bytes -> 90 RTP/A2DP bytes -> 94 L2CAP bytes -> 98 ACL bytes; no transport/RF/audio claim"
    );
}
