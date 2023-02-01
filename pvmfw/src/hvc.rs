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

//! Wrappers around calls to the hypervisor.

use crate::smccc::{self, checked_hvc64, checked_hvc64_expect_zero};
use log::info;

const ARM_SMCCC_KVM_FUNC_HYP_MEMINFO: u32 = 0xc6000002;
const ARM_SMCCC_KVM_FUNC_MEM_SHARE: u32 = 0xc6000003;
const ARM_SMCCC_KVM_FUNC_MEM_UNSHARE: u32 = 0xc6000004;
const VENDOR_HYP_KVM_MMIO_GUARD_INFO_FUNC_ID: u32 = 0xc6000005;
const VENDOR_HYP_KVM_MMIO_GUARD_ENROLL_FUNC_ID: u32 = 0xc6000006;
const VENDOR_HYP_KVM_MMIO_GUARD_MAP_FUNC_ID: u32 = 0xc6000007;
const VENDOR_HYP_KVM_MMIO_GUARD_UNMAP_FUNC_ID: u32 = 0xc6000008;

/// Queries the memory protection parameters for a protected virtual machine.
///
/// Returns the memory protection granule size in bytes.
pub fn hyp_meminfo() -> smccc::Result<u64> {
    let args = [0u64; 17];
    checked_hvc64(ARM_SMCCC_KVM_FUNC_HYP_MEMINFO, args)
}

/// Shares a region of memory with the KVM host, granting it read, write and execute permissions.
/// The size of the region is equal to the memory protection granule returned by [`hyp_meminfo`].
pub fn mem_share(base_ipa: u64) -> smccc::Result<()> {
    let mut args = [0u64; 17];
    args[0] = base_ipa;

    checked_hvc64_expect_zero(ARM_SMCCC_KVM_FUNC_MEM_SHARE, args)
}

/// Revokes access permission from the KVM host to a memory region previously shared with
/// [`mem_share`]. The size of the region is equal to the memory protection granule returned by
/// [`hyp_meminfo`].
pub fn mem_unshare(base_ipa: u64) -> smccc::Result<()> {
    let mut args = [0u64; 17];
    args[0] = base_ipa;

    checked_hvc64_expect_zero(ARM_SMCCC_KVM_FUNC_MEM_UNSHARE, args)
}

pub fn mmio_guard_info() -> smccc::Result<u64> {
    let args = [0u64; 17];

    checked_hvc64(VENDOR_HYP_KVM_MMIO_GUARD_INFO_FUNC_ID, args)
}

pub fn mmio_guard_enroll() -> smccc::Result<()> {
    let args = [0u64; 17];

    checked_hvc64_expect_zero(VENDOR_HYP_KVM_MMIO_GUARD_ENROLL_FUNC_ID, args)
}

pub fn mmio_guard_map(ipa: u64) -> smccc::Result<()> {
    let mut args = [0u64; 17];
    args[0] = ipa;

    // TODO(b/253586500): pKVM currently returns a i32 instead of a i64.
    let is_i32_error_code = |n| u32::try_from(n).ok().filter(|v| (*v as i32) < 0).is_some();
    match checked_hvc64_expect_zero(VENDOR_HYP_KVM_MMIO_GUARD_MAP_FUNC_ID, args) {
        Err(smccc::Error::Unexpected(e)) if is_i32_error_code(e) => {
            info!("Handled a pKVM bug by interpreting the MMIO_GUARD_MAP return value as i32");
            match e as u32 as i32 {
                -1 => Err(smccc::Error::NotSupported),
                -2 => Err(smccc::Error::NotRequired),
                -3 => Err(smccc::Error::InvalidParameter),
                ret => Err(smccc::Error::Unknown(ret as i64)),
            }
        }
        res => res,
    }
}

pub fn mmio_guard_unmap(ipa: u64) -> smccc::Result<()> {
    let mut args = [0u64; 17];
    args[0] = ipa;

    // TODO(b/251426790): pKVM currently returns NOT_SUPPORTED for SUCCESS.
    match checked_hvc64_expect_zero(VENDOR_HYP_KVM_MMIO_GUARD_UNMAP_FUNC_ID, args) {
        Err(smccc::Error::NotSupported) | Ok(_) => Ok(()),
        x => x,
    }
}
