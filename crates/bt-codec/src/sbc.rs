// SPDX-License-Identifier: GPL-3.0-or-later

use bt_core::{
    Error, SbcAllocation, SbcBitpoolRange, SbcBlocks, SbcChannelMode, SbcParameters, SbcSampleRate,
    SbcSubbands,
};
use core::ffi::c_void;
use core::marker::PhantomData;
use core::mem::{MaybeUninit, align_of};
use core::ptr::NonNull;

const OK: i32 = 0;
const UNSUPPORTED: i32 = -1;
const INVALID: i32 = -2;
const BUFFER_TOO_SMALL: i32 = -4;
const INTEGRITY_LOST: i32 = -7;
const MODE_STEREO: u32 = 0;
const MODE_JOINT_STEREO: u32 = 1;
const ALLOCATION_LOUDNESS: u32 = 0;

unsafe extern "C" {
    fn ps5_bt_sbc_storage_size() -> u32;
    fn ps5_bt_sbc_storage_alignment() -> u32;
    fn ps5_bt_sbc_init(
        storage: *mut c_void,
        storage_size: u32,
        sample_rate_hz: u32,
        channel_mode: u32,
        blocks: u32,
        subbands: u32,
        allocation: u32,
        bitpool: u32,
    ) -> i32;
    fn ps5_bt_sbc_encode(
        storage: *mut c_void,
        storage_size: u32,
        pcm: *const i16,
        pcm_samples: u32,
        output: *mut u8,
        output_capacity: u32,
        out_pcm_samples: *mut u32,
        out_bytes: *mut u32,
    ) -> i32;
    fn ps5_bt_sbc_finish(storage: *mut c_void, storage_size: u32) -> i32;
}

fn status(code: i32) -> Result<(), Error> {
    match code {
        OK => Ok(()),
        UNSUPPORTED => Err(Error::Unsupported),
        INVALID => Err(Error::Invalid),
        BUFFER_TOO_SMALL => Err(Error::BufferTooSmall),
        INTEGRITY_LOST => Err(Error::IntegrityLost),
        other => Err(Error::Backend(other)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageRequirements {
    pub size: usize,
    pub alignment: usize,
}

impl StorageRequirements {
    fn query() -> Result<Self, Error> {
        // SAFETY: argument-free integer queries; the linked wrapper retains nothing.
        let size = unsafe { ps5_bt_sbc_storage_size() } as usize;
        // SAFETY: argument-free integer queries; the linked wrapper retains nothing.
        let alignment = unsafe { ps5_bt_sbc_storage_alignment() } as usize;
        if size == 0 || alignment == 0 || !alignment.is_power_of_two() {
            return Err(Error::IntegrityLost);
        }
        Ok(Self { size, alignment })
    }
}

/// Bounded external storage for one encoder. Its size is caller policy, not a
/// copied definition of any upstream private structure.
#[repr(C, align(64))]
pub struct EncoderStorage<const N: usize> {
    bytes: [MaybeUninit<u8>; N],
}

impl<const N: usize> EncoderStorage<N> {
    pub const fn new() -> Self {
        Self {
            bytes: [MaybeUninit::uninit(); N],
        }
    }

    pub const fn capacity(&self) -> usize {
        N
    }
}

impl<const N: usize> Default for EncoderStorage<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncodedFrame {
    pub pcm_frames: usize,
    pub bytes: usize,
}

/// One mutable encoder borrowing stable caller storage for its entire lifetime.
/// The external borrow prevents the storage from moving; moving this small owner
/// is safe because C points only into the external storage. The raw-pointer marker
/// intentionally makes the owner neither `Send` nor `Sync`.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<bt_codec::Encoder<'static, 8192>>();
/// ```
///
/// ```compile_fail
/// fn require_sync<T: Sync>() {}
/// require_sync::<bt_codec::Encoder<'static, 8192>>();
/// ```
pub struct Encoder<'storage, const N: usize> {
    storage: NonNull<c_void>,
    storage_size: u32,
    parameters: SbcParameters,
    active: bool,
    poisoned: bool,
    _storage: PhantomData<&'storage mut EncoderStorage<N>>,
    _single_thread: PhantomData<*mut ()>,
}

impl<'storage, const N: usize> Encoder<'storage, N> {
    pub fn new(
        storage: &'storage mut EncoderStorage<N>,
        parameters: SbcParameters,
        negotiated_bitpool: SbcBitpoolRange,
    ) -> Result<Self, Error> {
        parameters.validate_negotiated_bitpool(negotiated_bitpool)?;
        let channel_mode = match parameters.channel_mode() {
            SbcChannelMode::Stereo => MODE_STEREO,
            SbcChannelMode::JointStereo => MODE_JOINT_STEREO,
            SbcChannelMode::Mono | SbcChannelMode::DualChannel => {
                return Err(Error::Unsupported);
            }
        };
        if parameters.sample_rate() != SbcSampleRate::Hz48000
            || parameters.blocks() != SbcBlocks::Sixteen
            || parameters.subbands() != SbcSubbands::Eight
            || parameters.allocation() != SbcAllocation::Loudness
            || parameters.bitpool() > 53
        {
            return Err(Error::Unsupported);
        }

        let requirements = StorageRequirements::query()?;
        if N < requirements.size || requirements.alignment > align_of::<EncoderStorage<N>>() {
            return Err(Error::BufferTooSmall);
        }
        let pointer = NonNull::from(&mut storage.bytes).cast::<c_void>();
        if pointer.as_ptr() as usize & (requirements.alignment - 1) != 0 {
            return Err(Error::Invalid);
        }
        let storage_size = u32::try_from(requirements.size).map_err(|_| Error::BufferTooSmall)?;
        let bitpool = u32::from(parameters.bitpool());
        // SAFETY: storage is exclusively borrowed for 'storage, has the queried
        // size/alignment, and C retains only pointers within that same borrow.
        status(unsafe {
            ps5_bt_sbc_init(
                pointer.as_ptr(),
                storage_size,
                parameters.sample_rate().hz(),
                channel_mode,
                parameters.blocks().count() as u32,
                parameters.subbands().count() as u32,
                ALLOCATION_LOUDNESS,
                bitpool,
            )
        })?;

        Ok(Self {
            storage: pointer,
            storage_size,
            parameters,
            active: true,
            poisoned: false,
            _storage: PhantomData,
            _single_thread: PhantomData,
        })
    }

    pub fn storage_requirements() -> Result<StorageRequirements, Error> {
        StorageRequirements::query()
    }

    pub const fn parameters(&self) -> SbcParameters {
        self.parameters
    }

    pub fn encode_frame(&mut self, pcm: &[i16], output: &mut [u8]) -> Result<EncodedFrame, Error> {
        if self.poisoned {
            return Err(Error::IntegrityLost);
        }
        if pcm.len() != self.parameters.pcm_samples() {
            return Err(Error::Invalid);
        }
        let expected_bytes = self.parameters.frame_length_bytes();
        if output.len() < expected_bytes {
            return Err(Error::BufferTooSmall);
        }
        let pcm_samples = u32::try_from(pcm.len()).map_err(|_| Error::Invalid)?;
        let output_capacity = u32::try_from(expected_bytes).map_err(|_| Error::BufferTooSmall)?;
        let mut consumed = 0;
        let mut written = 0;
        // SAFETY: the encoder owns its live C state; exact readable PCM and
        // writable output bounds are passed synchronously and never retained.
        let result = status(unsafe {
            ps5_bt_sbc_encode(
                self.storage.as_ptr(),
                self.storage_size,
                pcm.as_ptr(),
                pcm_samples,
                output.as_mut_ptr(),
                output_capacity,
                &mut consumed,
                &mut written,
            )
        });
        if let Err(error) = result {
            if error == Error::IntegrityLost {
                self.poisoned = true;
            }
            return Err(error);
        }
        if consumed as usize != pcm.len() || written as usize != expected_bytes {
            self.poisoned = true;
            return Err(Error::IntegrityLost);
        }
        Ok(EncodedFrame {
            pcm_frames: self.parameters.pcm_frames(),
            bytes: expected_bytes,
        })
    }

    pub fn finish(mut self) -> Result<(), Error> {
        self.finish_inner()
    }

    fn finish_inner(&mut self) -> Result<(), Error> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        // SAFETY: this owner consumes the one live C instance exactly once.
        status(unsafe { ps5_bt_sbc_finish(self.storage.as_ptr(), self.storage_size) })
    }
}

impl<const N: usize> Drop for Encoder<'_, N> {
    fn drop(&mut self) {
        let _ = self.finish_inner();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn unknown_statuses_retain_the_raw_backend_code() {
        assert_eq!(status(-5), Err(Error::Backend(-5)));
        assert_eq!(status(i32::MIN), Err(Error::Backend(i32::MIN)));
    }

    #[test]
    fn poisoned_owner_never_reenters_the_c_encoder() {
        let mut storage = EncoderStorage::<8192>::new();
        let mut encoder = Encoder::new(
            &mut storage,
            parameters(),
            SbcBitpoolRange::new(2, 53).unwrap(),
        )
        .unwrap();
        encoder.poisoned = true;
        let input = [0_i16; 256];
        let mut output = [0xa5_u8; 119];
        assert_eq!(
            encoder.encode_frame(&input, &mut output),
            Err(Error::IntegrityLost)
        );
        assert_eq!(output, [0xa5; 119]);
    }
}
