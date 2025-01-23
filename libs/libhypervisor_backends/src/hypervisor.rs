// Copyright 2023, The Android Open Source Project
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

//! Wrappers around hypervisor back-ends.

mod common;
mod geniezone;
mod gunyah;
#[cfg(target_arch = "aarch64")]
#[path = "hypervisor/kvm_aarch64.rs"]
mod kvm;

use super::{Error, Result};
use alloc::boxed::Box;
use common::Hypervisor;
pub use common::{DeviceAssigningHypervisor, MemSharingHypervisor, MmioGuardedHypervisor};
pub use geniezone::GeniezoneError;
use geniezone::GeniezoneHypervisor;
use gunyah::GunyahHypervisor;
pub use kvm::KvmError;
use kvm::{ProtectedKvmHypervisor, RegularKvmHypervisor};
use once_cell::race::OnceBox;
use smccc::hvc64;
use uuid::Uuid;

enum HypervisorBackend {
    RegularKvm,
    Gunyah,
    Geniezone,
    ProtectedKvm,
}

impl HypervisorBackend {
    fn get_hypervisor(&self) -> &'static dyn Hypervisor {
        match self {
            Self::RegularKvm => &RegularKvmHypervisor,
            Self::Gunyah => &GunyahHypervisor,
            Self::Geniezone => &GeniezoneHypervisor,
            Self::ProtectedKvm => &ProtectedKvmHypervisor,
        }
    }
}

impl TryFrom<Uuid> for HypervisorBackend {
    type Error = Error;

    fn try_from(uuid: Uuid) -> Result<HypervisorBackend> {
        match uuid {
            GeniezoneHypervisor::UUID => Ok(HypervisorBackend::Geniezone),
            GunyahHypervisor::UUID => Ok(HypervisorBackend::Gunyah),
            RegularKvmHypervisor::UUID => {
                // Protected KVM has the same UUID as "regular" KVM so issue an HVC that is assumed
                // to only be supported by pKVM: if it returns SUCCESS, deduce that this is pKVM
                // and if it returns NOT_SUPPORTED assume that it is "regular" KVM.
                match ProtectedKvmHypervisor.as_mmio_guard().unwrap().granule() {
                    Ok(_) => Ok(HypervisorBackend::ProtectedKvm),
                    Err(Error::KvmError(KvmError::NotSupported, _)) => {
                        Ok(HypervisorBackend::RegularKvm)
                    }
                    Err(e) => Err(e),
                }
            }
            u => Err(Error::UnsupportedHypervisorUuid(u)),
        }
    }
}

const ARM_SMCCC_VENDOR_HYP_CALL_UID_FUNC_ID: u32 = 0x8600ff01;

fn query_vendor_hyp_call_uid() -> Uuid {
    let args = [0u64; 17];
    let res = hvc64(ARM_SMCCC_VENDOR_HYP_CALL_UID_FUNC_ID, args);

    // KVM's UUID of "28b46fb6-2ec5-11e9-a9ca-4b564d003a74" is generated by
    // Uuid::from_u128() from an input value of
    // 0x28b46fb6_2ec511e9_a9ca4b56_4d003a74. ARM's SMC calling convention
    // (Document number ARM DEN 0028E) describes the UUID register mapping such
    // that W0 contains bytes 0..3 of UUID, with byte 0 in lower order bits. In
    // the KVM example, byte 0 of KVM's UUID (0x28) will be returned in the low
    // 8-bits of W0, while byte 15 (0x74) will be returned in bits 31-24 of W3.
    //
    // `uuid` value derived below thus need to be byte-reversed before
    // being used in Uuid::from_u128(). Alternately use Uuid::from_u128_le()
    // to achieve the same.

    let uuid = ((res[3] as u32 as u128) << 96)
        | ((res[2] as u32 as u128) << 64)
        | ((res[1] as u32 as u128) << 32)
        | (res[0] as u32 as u128);

    Uuid::from_u128_le(uuid)
}

fn detect_hypervisor() -> HypervisorBackend {
    query_vendor_hyp_call_uid().try_into().expect("Failed to detect hypervisor")
}

/// Gets the hypervisor singleton.
fn get_hypervisor() -> &'static dyn Hypervisor {
    static HYPERVISOR: OnceBox<HypervisorBackend> = OnceBox::new();

    HYPERVISOR.get_or_init(|| Box::new(detect_hypervisor())).get_hypervisor()
}

/// Gets the MMIO_GUARD hypervisor singleton, if any.
pub fn get_mmio_guard() -> Option<&'static dyn MmioGuardedHypervisor> {
    get_hypervisor().as_mmio_guard()
}

/// Gets the dynamic memory sharing hypervisor singleton, if any.
pub fn get_mem_sharer() -> Option<&'static dyn MemSharingHypervisor> {
    get_hypervisor().as_mem_sharer()
}

/// Gets the device assigning hypervisor singleton, if any.
pub fn get_device_assigner() -> Option<&'static dyn DeviceAssigningHypervisor> {
    get_hypervisor().as_device_assigner()
}
