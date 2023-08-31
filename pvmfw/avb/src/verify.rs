// Copyright 2022, The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! This module handles the pvmfw payload verification.

use crate::descriptor::{Descriptors, Digest};
use crate::ops::{Ops, Payload};
use crate::partition::PartitionName;
use crate::PvmfwVerifyError;
use alloc::vec;
use alloc::vec::Vec;
use avb_bindgen::{AvbPartitionData, AvbVBMetaData};
use core::ffi::c_char;

/// Verified data returned when the payload verification succeeds.
#[derive(Debug, PartialEq, Eq)]
pub struct VerifiedBootData<'a> {
    /// DebugLevel of the VM.
    pub debug_level: DebugLevel,
    /// Kernel digest.
    pub kernel_digest: Digest,
    /// Initrd digest if initrd exists.
    pub initrd_digest: Option<Digest>,
    /// Trusted public key.
    pub public_key: &'a [u8],
    /// VM capabilities.
    pub capabilities: Vec<Capability>,
}

/// This enum corresponds to the `DebugLevel` in `VirtualMachineConfig`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugLevel {
    /// Not debuggable at all.
    None,
    /// Fully debuggable.
    Full,
}

/// VM Capability.
#[derive(Debug, PartialEq, Eq)]
pub enum Capability {
    /// Remote attestation.
    RemoteAttest,
}

impl Capability {
    const KEY: &[u8] = b"com.android.virt.cap";
    const REMOTE_ATTEST: &[u8] = b"remote_attest";
    const SEPARATOR: u8 = b'|';

    fn get_capabilities(property_value: &[u8]) -> Result<Vec<Self>, PvmfwVerifyError> {
        let mut res = Vec::new();

        for v in property_value.split(|b| *b == Self::SEPARATOR) {
            let cap = match v {
                Self::REMOTE_ATTEST => Self::RemoteAttest,
                _ => return Err(PvmfwVerifyError::UnknownVbmetaProperty),
            };
            if res.contains(&cap) {
                return Err(avb::SlotVerifyError::InvalidMetadata.into());
            }
            res.push(cap);
        }
        Ok(res)
    }
}

fn verify_only_one_vbmeta_exists(
    vbmeta_images: &[AvbVBMetaData],
) -> Result<(), avb::SlotVerifyError> {
    if vbmeta_images.len() == 1 {
        Ok(())
    } else {
        Err(avb::SlotVerifyError::InvalidMetadata)
    }
}

fn verify_vbmeta_is_from_kernel_partition(
    vbmeta_image: &AvbVBMetaData,
) -> Result<(), avb::SlotVerifyError> {
    match (vbmeta_image.partition_name as *const c_char).try_into() {
        Ok(PartitionName::Kernel) => Ok(()),
        _ => Err(avb::SlotVerifyError::InvalidMetadata),
    }
}

fn verify_vbmeta_has_only_one_hash_descriptor(
    descriptors: &Descriptors,
) -> Result<(), avb::SlotVerifyError> {
    if descriptors.num_hash_descriptor() == 1 {
        Ok(())
    } else {
        Err(avb::SlotVerifyError::InvalidMetadata)
    }
}

fn verify_loaded_partition_has_expected_length(
    loaded_partitions: &[AvbPartitionData],
    partition_name: PartitionName,
    expected_len: usize,
) -> Result<(), avb::SlotVerifyError> {
    if loaded_partitions.len() != 1 {
        // Only one partition should be loaded in each verify result.
        return Err(avb::SlotVerifyError::Io);
    }
    let loaded_partition = loaded_partitions[0];
    if !PartitionName::try_from(loaded_partition.partition_name as *const c_char)
        .map_or(false, |p| p == partition_name)
    {
        // Only the requested partition should be loaded.
        return Err(avb::SlotVerifyError::Io);
    }
    if loaded_partition.data_size == expected_len {
        Ok(())
    } else {
        Err(avb::SlotVerifyError::Verification)
    }
}

/// Verifies that the vbmeta contains at most one property descriptor and it indicates the
/// vm type is service VM.
fn verify_property_and_get_capabilities(
    descriptors: &Descriptors,
) -> Result<Vec<Capability>, PvmfwVerifyError> {
    if !descriptors.has_property_descriptor() {
        return Ok(vec![]);
    }
    descriptors
        .find_property_value(Capability::KEY)
        .ok_or(PvmfwVerifyError::UnknownVbmetaProperty)
        .and_then(Capability::get_capabilities)
}

/// Verifies the payload (signed kernel + initrd) against the trusted public key.
pub fn verify_payload<'a>(
    kernel: &[u8],
    initrd: Option<&[u8]>,
    trusted_public_key: &'a [u8],
) -> Result<VerifiedBootData<'a>, PvmfwVerifyError> {
    let mut payload = Payload::new(kernel, initrd, trusted_public_key);
    let mut ops = Ops::from(&mut payload);
    let kernel_verify_result = ops.verify_partition(PartitionName::Kernel.as_cstr())?;

    let vbmeta_images = kernel_verify_result.vbmeta_images()?;
    verify_only_one_vbmeta_exists(vbmeta_images)?;
    let vbmeta_image = vbmeta_images[0];
    verify_vbmeta_is_from_kernel_partition(&vbmeta_image)?;
    // SAFETY: It is safe because the `vbmeta_image` is collected from `AvbSlotVerifyData`,
    // which is returned by `avb_slot_verify()` when the verification succeeds. It is
    // guaranteed by libavb to be non-null and to point to a valid VBMeta structure.
    let descriptors = unsafe { Descriptors::from_vbmeta(vbmeta_image)? };
    let capabilities = verify_property_and_get_capabilities(&descriptors)?;
    let kernel_descriptor = descriptors.find_hash_descriptor(PartitionName::Kernel)?;

    if initrd.is_none() {
        verify_vbmeta_has_only_one_hash_descriptor(&descriptors)?;
        return Ok(VerifiedBootData {
            debug_level: DebugLevel::None,
            kernel_digest: *kernel_descriptor.digest,
            initrd_digest: None,
            public_key: trusted_public_key,
            capabilities,
        });
    }

    let initrd = initrd.unwrap();
    let (debug_level, initrd_verify_result, initrd_partition_name) =
        if let Ok(result) = ops.verify_partition(PartitionName::InitrdNormal.as_cstr()) {
            (DebugLevel::None, result, PartitionName::InitrdNormal)
        } else if let Ok(result) = ops.verify_partition(PartitionName::InitrdDebug.as_cstr()) {
            (DebugLevel::Full, result, PartitionName::InitrdDebug)
        } else {
            return Err(avb::SlotVerifyError::Verification.into());
        };
    let loaded_partitions = initrd_verify_result.loaded_partitions()?;
    verify_loaded_partition_has_expected_length(
        loaded_partitions,
        initrd_partition_name,
        initrd.len(),
    )?;
    let initrd_descriptor = descriptors.find_hash_descriptor(initrd_partition_name)?;
    Ok(VerifiedBootData {
        debug_level,
        kernel_digest: *kernel_descriptor.digest,
        initrd_digest: Some(*initrd_descriptor.digest),
        public_key: trusted_public_key,
        capabilities,
    })
}
