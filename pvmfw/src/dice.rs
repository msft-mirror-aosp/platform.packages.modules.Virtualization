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

//! Support for DICE derivation and BCC generation.
extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use ciborium::cbor;
use ciborium::Value;
use core::mem::size_of;
use diced_open_dice::{
    bcc_handover_main_flow, hash, Config, DiceMode, Hash, InputValues, HIDDEN_SIZE,
};
use pvmfw_avb::{Capability, DebugLevel, Digest, VerifiedBootData};
use zerocopy::AsBytes;

const COMPONENT_NAME_KEY: i64 = -70002;
const SECURITY_VERSION_KEY: i64 = -70005;
const RKP_VM_MARKER_KEY: i64 = -70006;
// TODO(b/291245237): Document this key along with others used in ConfigDescriptor in AVF based VM.
const INSTANCE_HASH_KEY: i64 = -71003;

#[derive(Debug)]
pub enum Error {
    /// Error in CBOR operations
    CborError(ciborium::value::Error),
    /// Error in DICE operations
    DiceError(diced_open_dice::DiceError),
}

impl From<ciborium::value::Error> for Error {
    fn from(e: ciborium::value::Error) -> Self {
        Self::CborError(e)
    }
}

impl From<diced_open_dice::DiceError> for Error {
    fn from(e: diced_open_dice::DiceError) -> Self {
        Self::DiceError(e)
    }
}

// DICE in pvmfw result type.
type Result<T> = core::result::Result<T, Error>;

fn to_dice_mode(debug_level: DebugLevel) -> DiceMode {
    match debug_level {
        DebugLevel::None => DiceMode::kDiceModeNormal,
        DebugLevel::Full => DiceMode::kDiceModeDebug,
    }
}

fn to_dice_hash(verified_boot_data: &VerifiedBootData) -> Result<Hash> {
    let mut digests = [0u8; size_of::<Digest>() * 2];
    digests[..size_of::<Digest>()].copy_from_slice(&verified_boot_data.kernel_digest);
    if let Some(initrd_digest) = verified_boot_data.initrd_digest {
        digests[size_of::<Digest>()..].copy_from_slice(&initrd_digest);
    }
    Ok(hash(&digests)?)
}

pub struct PartialInputs {
    pub code_hash: Hash,
    pub auth_hash: Hash,
    pub mode: DiceMode,
    pub security_version: u64,
    pub rkp_vm_marker: bool,
}

impl PartialInputs {
    pub fn new(data: &VerifiedBootData) -> Result<Self> {
        let code_hash = to_dice_hash(data)?;
        let auth_hash = hash(data.public_key)?;
        let mode = to_dice_mode(data.debug_level);
        // We use rollback_index from vbmeta as the security_version field in dice certificate.
        let security_version = data.rollback_index;
        let rkp_vm_marker = data.has_capability(Capability::RemoteAttest);

        Ok(Self { code_hash, auth_hash, mode, security_version, rkp_vm_marker })
    }

    pub fn write_next_bcc(
        self,
        current_bcc_handover: &[u8],
        salt: &[u8; HIDDEN_SIZE],
        instance_hash: Option<Hash>,
        next_bcc: &mut [u8],
    ) -> Result<()> {
        let config = self
            .generate_config_descriptor(instance_hash)
            .map_err(|_| diced_open_dice::DiceError::InvalidInput)?;

        let dice_inputs = InputValues::new(
            self.code_hash,
            Config::Descriptor(&config),
            self.auth_hash,
            self.mode,
            self.make_hidden(salt)?,
        );
        let _ = bcc_handover_main_flow(current_bcc_handover, &dice_inputs, next_bcc)?;
        Ok(())
    }

    fn make_hidden(&self, salt: &[u8; HIDDEN_SIZE]) -> Result<[u8; HIDDEN_SIZE]> {
        // We want to make sure we get a different sealing CDI for:
        // - VMs with different salt values
        // - An RKP VM and any other VM (regardless of salt)
        // The hidden input for DICE affects the sealing CDI (but the values in the config
        // descriptor do not).
        // Since the hidden input has to be a fixed size, create it as a hash of the values we
        // want included.
        #[derive(AsBytes)]
        #[repr(C, packed)]
        struct HiddenInput {
            rkp_vm_marker: bool,
            salt: [u8; HIDDEN_SIZE],
        }
        // TODO(b/291213394): Include `defer_rollback_protection` flag in the Hidden Input to
        // differentiate the secrets in both cases.
        Ok(hash(HiddenInput { rkp_vm_marker: self.rkp_vm_marker, salt: *salt }.as_bytes())?)
    }

    fn generate_config_descriptor(&self, instance_hash: Option<Hash>) -> Result<Vec<u8>> {
        let mut config = Vec::with_capacity(4);
        config.push((cbor!(COMPONENT_NAME_KEY)?, cbor!("vm_entry")?));
        if cfg!(dice_changes) {
            config.push((cbor!(SECURITY_VERSION_KEY)?, cbor!(self.security_version)?));
        }
        if self.rkp_vm_marker {
            config.push((cbor!(RKP_VM_MARKER_KEY)?, Value::Null))
        }
        if let Some(instance_hash) = instance_hash {
            config.push((cbor!(INSTANCE_HASH_KEY)?, Value::from(instance_hash.as_slice())));
        }
        let config = Value::Map(config);
        Ok(cbor_util::serialize(&config).map_err(|e| {
            ciborium::value::Error::Custom(format!("Error in serialization: {e:?}"))
        })?)
    }
}

/// Flushes data caches over the provided address range.
///
/// # Safety
///
/// The provided address and size must be to an address range that is valid for read and write
/// (typically on the stack, .bss, .data, or provided BCC) from a single allocation
/// (e.g. stack array).
#[no_mangle]
#[cfg(not(test))]
unsafe extern "C" fn DiceClearMemory(
    _ctx: *mut core::ffi::c_void,
    size: usize,
    addr: *mut core::ffi::c_void,
) {
    use core::slice;
    use vmbase::memory::flushed_zeroize;

    // SAFETY: We require our caller to provide a valid range within a single object. The open-dice
    // always calls this on individual stack-allocated arrays which ensures that.
    let region = unsafe { slice::from_raw_parts_mut(addr as *mut u8, size) };
    flushed_zeroize(region)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ciborium::Value;
    use std::collections::HashMap;
    use std::vec;

    const COMPONENT_VERSION_KEY: i64 = -70003;
    const RESETTABLE_KEY: i64 = -70004;
    const BASE_VB_DATA: VerifiedBootData = VerifiedBootData {
        debug_level: DebugLevel::None,
        kernel_digest: [1u8; size_of::<Digest>()],
        initrd_digest: Some([2u8; size_of::<Digest>()]),
        public_key: b"public key",
        capabilities: vec![],
        rollback_index: 42,
    };
    const HASH: Hash = *b"sixtyfourbyteslongsentencearerarebutletsgiveitatrycantbethathard";

    #[test]
    fn base_data_conversion() {
        let vb_data = BASE_VB_DATA;
        let inputs = PartialInputs::new(&vb_data).unwrap();

        assert_eq!(inputs.mode, DiceMode::kDiceModeNormal);
        assert_eq!(inputs.security_version, 42);
        assert!(!inputs.rkp_vm_marker);

        // TODO(b/313608219): Consider checks for code_hash and possibly auth_hash.
    }

    #[test]
    fn debuggable_conversion() {
        let vb_data = VerifiedBootData { debug_level: DebugLevel::Full, ..BASE_VB_DATA };
        let inputs = PartialInputs::new(&vb_data).unwrap();

        assert_eq!(inputs.mode, DiceMode::kDiceModeDebug);
    }

    #[test]
    fn rkp_vm_conversion() {
        let vb_data =
            VerifiedBootData { capabilities: vec![Capability::RemoteAttest], ..BASE_VB_DATA };
        let inputs = PartialInputs::new(&vb_data).unwrap();

        assert!(inputs.rkp_vm_marker);
    }

    #[test]
    fn base_config_descriptor() {
        let vb_data = BASE_VB_DATA;
        let inputs = PartialInputs::new(&vb_data).unwrap();
        let config_map = decode_config_descriptor(&inputs, None);

        assert_eq!(config_map.get(&COMPONENT_NAME_KEY).unwrap().as_text().unwrap(), "vm_entry");
        assert_eq!(config_map.get(&COMPONENT_VERSION_KEY), None);
        assert_eq!(config_map.get(&RESETTABLE_KEY), None);
        if cfg!(dice_changes) {
            assert_eq!(
                config_map.get(&SECURITY_VERSION_KEY).unwrap().as_integer().unwrap(),
                42.into()
            );
        } else {
            assert_eq!(config_map.get(&SECURITY_VERSION_KEY), None);
        }
        assert_eq!(config_map.get(&RKP_VM_MARKER_KEY), None);
    }

    #[test]
    fn config_descriptor_with_rkp_vm() {
        let vb_data =
            VerifiedBootData { capabilities: vec![Capability::RemoteAttest], ..BASE_VB_DATA };
        let inputs = PartialInputs::new(&vb_data).unwrap();
        let config_map = decode_config_descriptor(&inputs, Some(HASH));

        assert!(config_map.get(&RKP_VM_MARKER_KEY).unwrap().is_null());
    }

    #[test]
    fn config_descriptor_with_instance_hash() {
        let vb_data =
            VerifiedBootData { capabilities: vec![Capability::RemoteAttest], ..BASE_VB_DATA };
        let inputs = PartialInputs::new(&vb_data).unwrap();
        let config_map = decode_config_descriptor(&inputs, Some(HASH));
        assert_eq!(*config_map.get(&INSTANCE_HASH_KEY).unwrap(), Value::from(HASH.as_slice()));
    }

    #[test]
    fn config_descriptor_without_instance_hash() {
        let vb_data =
            VerifiedBootData { capabilities: vec![Capability::RemoteAttest], ..BASE_VB_DATA };
        let inputs = PartialInputs::new(&vb_data).unwrap();
        let config_map = decode_config_descriptor(&inputs, None);
        assert!(!config_map.contains_key(&INSTANCE_HASH_KEY));
    }

    fn decode_config_descriptor(
        inputs: &PartialInputs,
        instance_hash: Option<Hash>,
    ) -> HashMap<i64, Value> {
        let config_descriptor = inputs.generate_config_descriptor(instance_hash).unwrap();

        let cbor_map =
            cbor_util::deserialize::<Value>(&config_descriptor).unwrap().into_map().unwrap();

        cbor_map
            .into_iter()
            .map(|(k, v)| ((k.into_integer().unwrap().try_into().unwrap()), v))
            .collect()
    }
}
