// SPDX-License-Identifier: GPL-3.0-or-later
//! All packets below are synthetic; none is a hardware observation.
use bt_core::Error;
use bt_hci::{
    AclPacket, BroadcastFlag, BufferSize, Command, CommandComplete, CommandStatus,
    CompletedPackets, ConnectionHandle, LocalExtendedFeatures, LocalSupportedCommands,
    LocalSupportedFeatures, Opcode, PacketBoundary, ParseError, READ_LOCAL_VERSION,
};

#[test]
fn commands_round_trip_in_little_endian_without_h4() {
    let opcode = Opcode::from_fields(4, 1).unwrap();
    assert_eq!(opcode, Opcode::READ_LOCAL_VERSION);
    assert_eq!((opcode.ogf(), opcode.ocf()), (4, 1));
    let command = Command::new(opcode, &[]).unwrap();
    let mut output = [0xaa; 4];
    assert_eq!(command.encode(&mut output), Ok(3));
    assert_eq!(&output[..3], &READ_LOCAL_VERSION);
    assert_eq!(output[3], 0xaa);
    assert_eq!(Command::parse(&output[..3]), Ok(command));
    assert_eq!(Opcode::from_fields(64, 0), Err(Error::Invalid));
    assert_eq!(Opcode::from_fields(1, 1024), Err(Error::Invalid));
    assert_eq!(Opcode::from_raw(0xffff).raw(), 0xffff);

    let parameters = [0x7a; 255];
    let command = Command::new(Opcode::from_raw(0x1234), &parameters).unwrap();
    let mut output = [0xaa; 258];
    assert_eq!(command.encode(&mut output), Ok(258));
    assert_eq!(&output[..3], &[0x34, 0x12, 255]);
    assert_eq!(Command::parse(&output), Ok(command));
    assert_eq!(Command::new(opcode, &[0; 256]), Err(Error::Invalid));
    let mut short = [0xaa; 257];
    assert_eq!(command.encode(&mut short), Err(Error::BufferTooSmall));
    assert_eq!(short, [0xaa; 257]);
}

#[test]
fn all_command_header_lengths_are_exact() {
    let mut packet = [0; 259];
    for length in 0..=255_u8 {
        packet[2] = length;
        for actual in 0..=packet.len() {
            assert_eq!(
                Command::parse(&packet[..actual]).is_ok(),
                actual == usize::from(length) + 3
            );
        }
    }
}

#[test]
fn acl_handles_flags_and_byte_order_are_checked() {
    for raw in 0..=u16::MAX {
        assert_eq!(ConnectionHandle::new(raw).is_ok(), raw <= 0x0eff);
    }
    let handle = ConnectionHandle::new(0x0abc).unwrap();
    let payload = [0x12, 0x34, 0x56];
    let packet = AclPacket::new(
        handle,
        PacketBoundary::StartFlushable,
        BroadcastFlag::PointToPoint,
        &payload,
    )
    .unwrap();
    let mut output = [0xaa; 8];
    assert_eq!(packet.encode(&mut output), Ok(7));
    assert_eq!(output, [0xbc, 0x2a, 3, 0, 0x12, 0x34, 0x56, 0xaa]);
    assert_eq!(AclPacket::parse(&output[..7]), Ok(packet));
    assert_eq!(packet.handle(), handle);
    assert_eq!(packet.boundary(), PacketBoundary::StartFlushable);
    assert_eq!(packet.broadcast(), BroadcastFlag::PointToPoint);
    assert_eq!(packet.payload(), payload);
    for high in 0..=15_u8 {
        let bytes = [0xbc, (high << 4) | 0xa, 0, 0];
        let boundary = high & 3;
        let broadcast = high >> 2;
        assert_eq!(
            AclPacket::parse(&bytes).is_ok(),
            boundary != 3 && broadcast < 2 && !(boundary == 0 && broadcast == 1)
        );
    }
    assert_eq!(
        AclPacket::parse(&[0, 0x0f, 0, 0]),
        Err(ParseError::InvalidHandle)
    );
    assert_eq!(
        AclPacket::new(
            handle,
            PacketBoundary::StartNonFlushable,
            BroadcastFlag::ActivePeripheral,
            &[],
        ),
        Err(Error::Invalid)
    );
    let mut short = [0xaa; 6];
    assert_eq!(packet.encode(&mut short), Err(Error::BufferTooSmall));
    assert_eq!(short, [0xaa; 6]);
}

#[test]
fn acl_length_high_byte_maximum_oversize_and_truncations() {
    let payload = [0; 65536];
    let handle = ConnectionHandle::new(0).unwrap();
    assert_eq!(
        AclPacket::new(
            handle,
            PacketBoundary::Continuation,
            BroadcastFlag::PointToPoint,
            &payload
        ),
        Err(Error::Invalid)
    );
    let packet = AclPacket::new(
        handle,
        PacketBoundary::Continuation,
        BroadcastFlag::PointToPoint,
        &payload[..65535],
    )
    .unwrap();
    let mut encoded = [0; 65540];
    assert_eq!(packet.encode(&mut encoded), Ok(65539));
    assert_eq!(&encoded[..4], &[0, 0x10, 0xff, 0xff]);
    for actual in 0..65539 {
        assert_eq!(
            AclPacket::parse(&encoded[..actual]),
            Err(ParseError::Length)
        );
    }
    assert_eq!(AclPacket::parse(&encoded[..65539]), Ok(packet));
    assert_eq!(AclPacket::parse(&encoded), Err(ParseError::Length));
    encoded[2..4].copy_from_slice(&0x1234_u16.to_le_bytes());
    assert_eq!(
        AclPacket::parse(&encoded[..0x1238])
            .unwrap()
            .payload()
            .len(),
        0x1234
    );
    encoded[2] = 0x33;
    assert_eq!(
        AclPacket::parse(&encoded[..0x1238]),
        Err(ParseError::Length)
    );
}

fn complete(opcode: Opcode, returns: &[u8]) -> Vec<u8> {
    let mut packet = vec![0x0e, (3 + returns.len()) as u8, 7];
    packet.extend_from_slice(&opcode.raw().to_le_bytes());
    packet.extend_from_slice(returns);
    packet
}

#[test]
fn capability_decoders_preserve_raw_observations() {
    let mut returns = [0; 65];
    returns[1] = 0x01;
    returns[64] = 0x80;
    let commands =
        LocalSupportedCommands::parse(&complete(Opcode::READ_LOCAL_SUPPORTED_COMMANDS, &returns))
            .unwrap();
    assert_eq!(commands.command_credits, 7);
    assert_eq!(commands.commands[0], 1);
    assert_eq!(commands.commands[63], 0x80);
    let returns = [0, 1, 2, 3, 4, 5, 6, 7, 0x80];
    assert_eq!(
        LocalSupportedFeatures::parse(&complete(Opcode::READ_LOCAL_SUPPORTED_FEATURES, &returns)),
        Ok(LocalSupportedFeatures {
            command_credits: 7,
            features: [1, 2, 3, 4, 5, 6, 7, 0x80]
        })
    );
    assert_eq!(
        LocalExtendedFeatures::parse(&complete(
            Opcode::READ_LOCAL_EXTENDED_FEATURES,
            &[0, 2, 3, 1, 2, 3, 4, 5, 6, 7, 8]
        )),
        Ok(LocalExtendedFeatures {
            command_credits: 7,
            page: 2,
            max_page: 3,
            features: [1, 2, 3, 4, 5, 6, 7, 8]
        })
    );
    assert_eq!(
        BufferSize::parse(&complete(
            Opcode::READ_BUFFER_SIZE,
            &[0, 0x34, 0x12, 0x56, 0x78, 0x9a, 0xbc, 0xde]
        )),
        Ok(BufferSize {
            command_credits: 7,
            acl_packet_length: 0x1234,
            synchronous_packet_length: 0x56,
            total_acl_packets: 0x9a78,
            total_synchronous_packets: 0xdebc
        })
    );
    assert_eq!(
        BufferSize::parse(&complete(Opcode::READ_BUFFER_SIZE, &[0; 8]))
            .unwrap()
            .total_acl_packets,
        0
    );
}

#[test]
fn every_capability_truncation_extra_parameter_and_failure_is_checked() {
    // Function pointers discard values only after exercising the complete parser.
    type Parser = fn(&[u8]) -> Result<(), ParseError>;
    let cases: [(Opcode, usize, Parser); 4] = [
        (Opcode::READ_LOCAL_SUPPORTED_COMMANDS, 65, |b| {
            LocalSupportedCommands::parse(b).map(|_| ())
        }),
        (Opcode::READ_LOCAL_SUPPORTED_FEATURES, 9, |b| {
            LocalSupportedFeatures::parse(b).map(|_| ())
        }),
        (Opcode::READ_LOCAL_EXTENDED_FEATURES, 11, |b| {
            LocalExtendedFeatures::parse(b).map(|_| ())
        }),
        (Opcode::READ_BUFFER_SIZE, 8, |b| {
            BufferSize::parse(b).map(|_| ())
        }),
    ];
    for (opcode, length, parser) in cases {
        let bytes = complete(opcode, &vec![0; length]);
        assert_eq!(parser(&bytes), Ok(()));
        for n in 0..bytes.len() {
            assert!(parser(&bytes[..n]).is_err());
        }
        for n in 0..length {
            assert_eq!(
                parser(&complete(opcode, &vec![0; n])),
                Err(ParseError::Length)
            );
        }
        assert_eq!(
            parser(&complete(opcode, &vec![0; length + 1])),
            Err(ParseError::Length)
        );
        assert_eq!(
            parser(&complete(opcode, &[0x0c])),
            Err(ParseError::ControllerStatus(0x0c))
        );
        assert_eq!(
            parser(&complete(Opcode::from_raw(0xffff), &[0x0c])),
            Err(ParseError::UnexpectedOpcode)
        );
        let mut foreign = bytes.clone();
        foreign[0] = 0xff;
        assert_eq!(parser(&foreign), Err(ParseError::UnexpectedEvent));
    }
}

#[test]
fn generic_completions_status_and_opcode_zero_remain_raw() {
    assert_eq!(
        CommandComplete::parse(&[0x0e, 3, 0, 0, 0]),
        Ok(CommandComplete {
            command_credits: 0,
            opcode: Opcode::from_raw(0),
            return_parameters: &[]
        })
    );
    assert_eq!(
        CommandComplete::parse(&[0x0e, 2, 1, 2]),
        Err(ParseError::Length)
    );
    let packet = [0x0f, 4, 0x0c, 5, 0x34, 0x12];
    assert_eq!(
        CommandStatus::parse(&packet),
        Ok(CommandStatus {
            status: 0x0c,
            command_credits: 5,
            opcode: Opcode::from_raw(0x1234)
        })
    );
    for n in 0..packet.len() {
        assert!(CommandStatus::parse(&packet[..n]).is_err());
    }
    assert_eq!(
        CommandStatus::parse(&[0x0f, 3, 0, 0, 0]),
        Err(ParseError::Length)
    );
    assert_eq!(
        CommandStatus::parse(&[0x0f, 5, 0, 0, 0, 0, 0]),
        Err(ParseError::Length)
    );
    assert_eq!(
        CommandStatus::parse(&[0x0e, 4, 0, 0, 0, 0]),
        Err(ParseError::UnexpectedEvent)
    );
}

#[test]
fn completion_entries_preserve_foreign_handles_without_credit_aggregation() {
    let bytes = [0x13, 9, 2, 0x34, 0x02, 0x78, 0x56, 0xff, 0x0e, 0xff, 0xff];
    let completions = CompletedPackets::parse(&bytes).unwrap();
    assert_eq!(completions.len(), 2);
    assert!(!completions.is_empty());
    let mut iter = completions.iter();
    let own = iter.next().unwrap();
    assert_eq!((own.handle.raw(), own.packets), (0x0234, 0x5678));
    let foreign = iter.next().unwrap();
    assert_eq!((foreign.handle.raw(), foreign.packets), (0x0eff, 0xffff));
    assert_eq!(iter.next(), None);
    assert!(CompletedPackets::parse(&[0x13, 1, 0]).unwrap().is_empty());
    for n in 0..bytes.len() {
        assert!(CompletedPackets::parse(&bytes[..n]).is_err());
    }
    let mut invalid = bytes;
    invalid[8] = 0x1e; // Upper handle bits must not be masked into an owned handle.
    assert_eq!(
        CompletedPackets::parse(&invalid),
        Err(ParseError::InvalidHandle)
    );
    for count in 0..=255_u8 {
        invalid = bytes;
        invalid[2] = count;
        assert_eq!(CompletedPackets::parse(&invalid).is_ok(), count == 2);
    }
    assert_eq!(CompletedPackets::parse(&[0x13, 0]), Err(ParseError::Length));
    assert_eq!(
        CompletedPackets::parse(&[0xff, 1, 0]),
        Err(ParseError::UnexpectedEvent)
    );
}
