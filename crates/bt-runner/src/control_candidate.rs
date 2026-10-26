// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed Rust-only storage/configuration for an unavailable-radio control probe.
use crate::{Config, LoopBuffers};
use bt_app::Storage;
use bt_discovery::ResultSlot;

pub struct ControlStorage<const N: usize> {
    channel: [u8; 512],
    sdp: [u8; 512],
    media: [u8; 512],
    avdtp: [u8; 512],
    slots: [ResultSlot; N],
    input: [u8; 1024],
    offer: [u8; 1024],
    sdp_scratch: [u8; 512],
    rtp: [u8; 1024],
    basic: [u8; 1024],
    acl: [u8; 1028],
}
impl<const N: usize> Default for ControlStorage<N> {
    fn default() -> Self {
        Self::new()
    }
}
pub struct ControlStorageParts<'a> {
    pub application: Storage<'a>,
    pub discovery: &'a mut [ResultSlot],
    pub work: LoopBuffers<'a>,
}
impl<const N: usize> ControlStorage<N> {
    pub const fn new() -> Self {
        Self {
            channel: [0; 512],
            sdp: [0; 512],
            media: [0; 512],
            avdtp: [0; 512],
            slots: [ResultSlot::empty(); N],
            input: [0; 1024],
            offer: [0; 1024],
            sdp_scratch: [0; 512],
            rtp: [0; 1024],
            basic: [0; 1024],
            acl: [0; 1028],
        }
    }
    pub fn parts(&mut self) -> ControlStorageParts<'_> {
        ControlStorageParts {
            application: Storage {
                channel_tx: &mut self.channel,
                sdp: &mut self.sdp,
                media_pdu: &mut self.media,
                avdtp: &mut self.avdtp,
            },
            discovery: &mut self.slots,
            work: LoopBuffers {
                input: &mut self.input,
                offer: &mut self.offer,
                sdp: &mut self.sdp_scratch,
                rtp: &mut self.rtp,
                basic: &mut self.basic,
                acl: &mut self.acl,
            },
        }
    }
}
/// Parser/policy configuration for a control-only experiment. It NEVER grants
/// command/ACL allowance, creates a native connection or enables capture/codec.
/// The disabled 40-byte ACL envelope only validates Rust storage contracts.
pub fn control_application_config() -> bt_app::Config {
    bt_app::Config {
        link: bt_link::Config {
            packet_types: 0x3306,
            page_scan_repetition: 1,
            allow_role_switch: false,
            connection_timeout_us: 1_000_000,
            security_timeout_us: 1_000_000,
            cleanup_timeout_us: 100_000,
            minimum_key_size: 16,
        },
        channels: bt_channel::ChannelConfig {
            local_cids: [0x40, 0x42, 0x44],
            local_mtu: 672,
            channel_timeout_us: 100_000,
            discovery_timeout_us: 100_000,
            cleanup_timeout_us: 100_000,
            discovery_limits: bt_sdp::DiscoveryLimits {
                max_chunk_bytes: 64,
                max_total_bytes: 256,
                max_nodes: 64,
                max_depth: 8,
                max_chunks: 8,
            },
        },
        source: bt_session::Config {
            signal_timeout_us: 100_000,
            media_open_timeout_us: 100_000,
            stop_timeout_us: 100_000,
            bitpool: 32,
            payload_type: 96,
            ssrc: 1,
            first_sequence: 0,
            first_timestamp: 0,
        },
        local_endpoint: bt_avdtp::StreamEndpointId::new(2).expect("constant nonzero endpoint"),
        reassembly: bt_avdtp::ReassemblyLimits {
            max_packets: 4,
            max_packet_bytes: 672,
            timeout_us: 100_000,
        },
        acl_data_max: 40,
        disconnect_timeout_us: 150_000,
        media_pdu_timeout_us: 100_000,
    }
}
pub fn control_runner_config() -> Config {
    Config {
        session: 1,
        first_scan_epoch: 1,
        first_link_epoch: 1,
        result_valid_us: 1_000_000,
        inquiry: bt_discovery::Config {
            inquiry_length: 1,
            max_responses: 8,
            scan_timeout_us: 1_000_000,
            cleanup_timeout_us: 100_000,
            max_result_events: 8,
        },
    }
}
pub fn control_recovery_policy() -> bt_core::recovery::Policy {
    bt_core::recovery::Policy {
        max_attempts: 2,
        episode_timeout_us: 1_000_000,
        initial_backoff_us: 1_000,
        max_backoff_us: 10_000,
        stable_success_us: 100_000,
    }
}
