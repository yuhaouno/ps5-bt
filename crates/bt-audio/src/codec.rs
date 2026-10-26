// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared persistent codec adapter; archive selection stays in bt-codec.
use crate::{BlockEncoder, FinishEncoder};
use bt_core::{Error, SbcParameters};

/// Reuses the existing reviewed encoder owner and caller interleaving storage.
/// `Encoder` still exclusively owns its C storage and remains !Send/!Sync.
pub struct SbcEncoder<'encoder, 'scratch, const N: usize> {
    encoder: bt_codec::Encoder<'encoder, N>,
    interleaved: &'scratch mut [i16],
}

impl<'encoder, 'scratch, const N: usize> SbcEncoder<'encoder, 'scratch, N> {
    pub fn new(
        encoder: bt_codec::Encoder<'encoder, N>,
        interleaved: &'scratch mut [i16],
    ) -> Result<Self, Error> {
        let samples = encoder.parameters().pcm_samples();
        if interleaved.len() < samples {
            return Err(Error::BufferTooSmall);
        }
        Ok(Self {
            encoder,
            interleaved: &mut interleaved[..samples],
        })
    }

    /// Consume the adapter and perform the existing encoder's exactly-once finish.
    pub fn finish(self) -> Result<(), Error> {
        self.encoder.finish()
    }
}

impl<const N: usize> BlockEncoder for SbcEncoder<'_, '_, N> {
    fn parameters(&self) -> SbcParameters {
        self.encoder.parameters()
    }

    fn encode_stereo(&mut self, pcm: &[[i16; 2]], output: &mut [u8]) -> Result<usize, Error> {
        if pcm.len() != self.encoder.parameters().pcm_frames() {
            return Err(Error::Invalid);
        }
        for (frame, samples) in pcm.iter().zip(self.interleaved.chunks_exact_mut(2)) {
            samples.copy_from_slice(frame);
        }
        self.encoder
            .encode_frame(self.interleaved, output)
            .map(|frame| frame.bytes)
    }
}

impl<const N: usize> FinishEncoder for SbcEncoder<'_, '_, N> {
    fn finish(self) -> Result<(), Error> {
        SbcEncoder::finish(self)
    }
}
