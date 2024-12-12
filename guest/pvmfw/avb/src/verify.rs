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

use crate::ops::{Ops, Payload};
use crate::partition::PartitionName;
use crate::PvmfwVerifyError;
use alloc::vec::Vec;
use avb::{
    Descriptor, DescriptorError, DescriptorResult, HashDescriptor, PartitionData, SlotVerifyError,
    SlotVerifyNoDataResult, VbmetaData,
};
use core::str;

// We use this for the rollback_index field if SlotVerifyData has empty rollback_indexes
const DEFAULT_ROLLBACK_INDEX: u64 = 0;

/// SHA256 digest type for kernel and initrd.
pub type Digest = [u8; 32];

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
    /// Rollback index of kernel.
    pub rollback_index: u64,
    /// Page size of kernel, if present.
    pub page_size: Option<usize>,
}

impl VerifiedBootData<'_> {
    /// Returns whether the kernel have the given capability
    pub fn has_capability(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
    }
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
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Capability {
    /// Remote attestation.
    RemoteAttest,
    /// Secretkeeper protected secrets.
    SecretkeeperProtection,
    /// Trusty security VM.
    TrustySecurityVm,
    /// UEFI support for booting guest kernel.
    SupportsUefiBoot,
    /// (internal)
    #[allow(non_camel_case_types)] // TODO: Use mem::variant_count once stable.
    _VARIANT_COUNT,
}

impl Capability {
    const KEY: &'static str = "com.android.virt.cap";
    const REMOTE_ATTEST: &'static [u8] = b"remote_attest";
    const TRUSTY_SECURITY_VM: &'static [u8] = b"trusty_security_vm";
    const SECRETKEEPER_PROTECTION: &'static [u8] = b"secretkeeper_protection";
    const SEPARATOR: u8 = b'|';
    const SUPPORTS_UEFI_BOOT: &'static [u8] = b"supports_uefi_boot";
    /// Number of supported capabilites.
    pub const COUNT: usize = Self::_VARIANT_COUNT as usize;

    /// Returns the capabilities indicated in `descriptor`, or error if the descriptor has
    /// unexpected contents.
    fn get_capabilities(vbmeta_data: &VbmetaData) -> Result<Vec<Self>, PvmfwVerifyError> {
        let Some(value) = vbmeta_data.get_property_value(Self::KEY) else {
            return Ok(Vec::new());
        };

        let mut res = Vec::new();

        for v in value.split(|b| *b == Self::SEPARATOR) {
            let cap = match v {
                Self::REMOTE_ATTEST => Self::RemoteAttest,
                Self::TRUSTY_SECURITY_VM => Self::TrustySecurityVm,
                Self::SECRETKEEPER_PROTECTION => Self::SecretkeeperProtection,
                Self::SUPPORTS_UEFI_BOOT => Self::SupportsUefiBoot,
                _ => return Err(PvmfwVerifyError::UnknownVbmetaProperty),
            };
            if res.contains(&cap) {
                return Err(SlotVerifyError::InvalidMetadata.into());
            }
            res.push(cap);
        }
        Ok(res)
    }
}

fn verify_only_one_vbmeta_exists(vbmeta_data: &[VbmetaData]) -> SlotVerifyNoDataResult<()> {
    if vbmeta_data.len() == 1 {
        Ok(())
    } else {
        Err(SlotVerifyError::InvalidMetadata)
    }
}

fn verify_vbmeta_is_from_kernel_partition(vbmeta_image: &VbmetaData) -> SlotVerifyNoDataResult<()> {
    match vbmeta_image.partition_name().try_into() {
        Ok(PartitionName::Kernel) => Ok(()),
        _ => Err(SlotVerifyError::InvalidMetadata),
    }
}

fn verify_loaded_partition_has_expected_length(
    loaded_partitions: &[PartitionData],
    partition_name: PartitionName,
    expected_len: usize,
) -> SlotVerifyNoDataResult<()> {
    if loaded_partitions.len() != 1 {
        // Only one partition should be loaded in each verify result.
        return Err(SlotVerifyError::Io);
    }
    let loaded_partition = &loaded_partitions[0];
    if !PartitionName::try_from(loaded_partition.partition_name())
        .map_or(false, |p| p == partition_name)
    {
        // Only the requested partition should be loaded.
        return Err(SlotVerifyError::Io);
    }
    if loaded_partition.data().len() == expected_len {
        Ok(())
    } else {
        Err(SlotVerifyError::Verification(None))
    }
}

/// Hash descriptors extracted from a vbmeta image.
///
/// We always have a kernel hash descriptor and may have initrd normal or debug descriptors.
struct HashDescriptors<'a> {
    kernel: &'a HashDescriptor<'a>,
    initrd_normal: Option<&'a HashDescriptor<'a>>,
    initrd_debug: Option<&'a HashDescriptor<'a>>,
}

impl<'a> HashDescriptors<'a> {
    /// Extracts the hash descriptors from all vbmeta descriptors. Any unexpected hash descriptor
    /// is an error.
    fn get(descriptors: &'a [Descriptor<'a>]) -> DescriptorResult<Self> {
        let mut kernel = None;
        let mut initrd_normal = None;
        let mut initrd_debug = None;

        for descriptor in descriptors.iter().filter_map(|d| match d {
            Descriptor::Hash(h) => Some(h),
            _ => None,
        }) {
            let target = match descriptor
                .partition_name
                .as_bytes()
                .try_into()
                .map_err(|_| DescriptorError::InvalidContents)?
            {
                PartitionName::Kernel => &mut kernel,
                PartitionName::InitrdNormal => &mut initrd_normal,
                PartitionName::InitrdDebug => &mut initrd_debug,
            };

            if target.is_some() {
                // Duplicates of the same partition name is an error.
                return Err(DescriptorError::InvalidContents);
            }
            target.replace(descriptor);
        }

        // Kernel is required, the others are optional.
        Ok(Self {
            kernel: kernel.ok_or(DescriptorError::InvalidContents)?,
            initrd_normal,
            initrd_debug,
        })
    }

    /// Returns an error if either initrd descriptor exists.
    fn verify_no_initrd(&self) -> Result<(), PvmfwVerifyError> {
        match self.initrd_normal.or(self.initrd_debug) {
            Some(_) => Err(SlotVerifyError::InvalidMetadata.into()),
            None => Ok(()),
        }
    }
}

/// Returns a copy of the SHA256 digest in `descriptor`, or error if the sizes don't match.
fn copy_digest(descriptor: &HashDescriptor) -> SlotVerifyNoDataResult<Digest> {
    let mut digest = Digest::default();
    if descriptor.digest.len() != digest.len() {
        return Err(SlotVerifyError::InvalidMetadata);
    }
    digest.clone_from_slice(descriptor.digest);
    Ok(digest)
}

/// Returns the indicated payload page size, if present.
fn read_page_size(vbmeta_data: &VbmetaData) -> Result<Option<usize>, PvmfwVerifyError> {
    let Some(property) = vbmeta_data.get_property_value("com.android.virt.page_size") else {
        return Ok(None);
    };
    let size = str::from_utf8(property)
        .or(Err(PvmfwVerifyError::InvalidPageSize))?
        .parse::<usize>()
        .or(Err(PvmfwVerifyError::InvalidPageSize))?
        .checked_mul(1024)
        // TODO(stable(unsigned_is_multiple_of)): use .is_multiple_of()
        .filter(|sz| sz % (4 << 10) == 0 && *sz != 0)
        .ok_or(PvmfwVerifyError::InvalidPageSize)?;

    Ok(Some(size))
}

/// Verifies the given initrd partition, and checks that the resulting contents looks like expected.
fn verify_initrd(
    ops: &mut Ops,
    partition_name: PartitionName,
    expected_initrd: &[u8],
) -> SlotVerifyNoDataResult<()> {
    let result =
        ops.verify_partition(partition_name.as_cstr()).map_err(|e| e.without_verify_data())?;
    verify_loaded_partition_has_expected_length(
        result.partition_data(),
        partition_name,
        expected_initrd.len(),
    )
}

/// Verifies the payload (signed kernel + initrd) against the trusted public key.
pub fn verify_payload<'a>(
    kernel: &[u8],
    initrd: Option<&[u8]>,
    trusted_public_key: &'a [u8],
) -> Result<VerifiedBootData<'a>, PvmfwVerifyError> {
    let payload = Payload::new(kernel, initrd, trusted_public_key);
    let mut ops = Ops::new(&payload);
    let kernel_verify_result = ops.verify_partition(PartitionName::Kernel.as_cstr())?;

    let vbmeta_images = kernel_verify_result.vbmeta_data();
    // TODO(b/302093437): Use explicit rollback_index_location instead of default
    // location (first element).
    let rollback_index =
        *kernel_verify_result.rollback_indexes().first().unwrap_or(&DEFAULT_ROLLBACK_INDEX);
    verify_only_one_vbmeta_exists(vbmeta_images)?;
    let vbmeta_image = &vbmeta_images[0];
    verify_vbmeta_is_from_kernel_partition(vbmeta_image)?;
    let descriptors = vbmeta_image.descriptors()?;
    let hash_descriptors = HashDescriptors::get(&descriptors)?;
    let capabilities = Capability::get_capabilities(vbmeta_image)?;
    let page_size = read_page_size(vbmeta_image)?;

    if initrd.is_none() {
        hash_descriptors.verify_no_initrd()?;
        return Ok(VerifiedBootData {
            debug_level: DebugLevel::None,
            kernel_digest: copy_digest(hash_descriptors.kernel)?,
            initrd_digest: None,
            public_key: trusted_public_key,
            capabilities,
            rollback_index,
            page_size,
        });
    }

    let initrd = initrd.unwrap();
    let (debug_level, initrd_descriptor) =
        if verify_initrd(&mut ops, PartitionName::InitrdNormal, initrd).is_ok() {
            (DebugLevel::None, hash_descriptors.initrd_normal)
        } else if verify_initrd(&mut ops, PartitionName::InitrdDebug, initrd).is_ok() {
            (DebugLevel::Full, hash_descriptors.initrd_debug)
        } else {
            return Err(SlotVerifyError::Verification(None).into());
        };
    let initrd_descriptor = initrd_descriptor.ok_or(DescriptorError::InvalidContents)?;
    Ok(VerifiedBootData {
        debug_level,
        kernel_digest: copy_digest(hash_descriptors.kernel)?,
        initrd_digest: Some(copy_digest(initrd_descriptor)?),
        public_key: trusted_public_key,
        capabilities,
        rollback_index,
        page_size,
    })
}
