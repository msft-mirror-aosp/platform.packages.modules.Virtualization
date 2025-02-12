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

//! pVM firmware.

#![no_main]
#![no_std]

extern crate alloc;

mod arch;
mod bcc;
mod bootargs;
mod config;
mod device_assignment;
mod dice;
mod entry;
mod fdt;
mod gpt;
mod instance;
mod memory;
mod rollback;

use crate::bcc::Bcc;
use crate::dice::PartialInputs;
use crate::entry::RebootReason;
use crate::fdt::{modify_for_next_stage, read_instance_id, sanitize_device_tree};
use crate::rollback::perform_rollback_protection;
use alloc::borrow::Cow;
use alloc::boxed::Box;
use bssl_avf::Digester;
use diced_open_dice::{bcc_handover_parse, DiceArtifacts, DiceContext, Hidden, VM_KEY_ALGORITHM};
use hypervisor_backends::get_mem_sharer;
use libfdt::Fdt;
use log::{debug, error, info, trace, warn};
use pvmfw_avb::verify_payload;
use pvmfw_avb::DebugLevel;
use pvmfw_embedded_key::PUBLIC_KEY;
use vmbase::heap;
use vmbase::memory::{flush, SIZE_4KB};
use vmbase::rand;

fn main<'a>(
    untrusted_fdt: &mut Fdt,
    signed_kernel: &[u8],
    ramdisk: Option<&[u8]>,
    current_bcc_handover: &[u8],
    mut debug_policy: Option<&[u8]>,
    vm_dtbo: Option<&mut [u8]>,
    vm_ref_dt: Option<&[u8]>,
) -> Result<(&'a [u8], bool), RebootReason> {
    info!("pVM firmware");
    debug!("FDT: {:?}", untrusted_fdt.as_ptr());
    debug!("Signed kernel: {:?} ({:#x} bytes)", signed_kernel.as_ptr(), signed_kernel.len());
    debug!("AVB public key: addr={:?}, size={:#x} ({1})", PUBLIC_KEY.as_ptr(), PUBLIC_KEY.len());
    if let Some(rd) = ramdisk {
        debug!("Ramdisk: {:?} ({:#x} bytes)", rd.as_ptr(), rd.len());
    } else {
        debug!("Ramdisk: None");
    }

    let bcc_handover = bcc_handover_parse(current_bcc_handover).map_err(|e| {
        error!("Invalid BCC Handover: {e:?}");
        RebootReason::InvalidBcc
    })?;
    trace!("BCC: {bcc_handover:x?}");

    let bcc = Bcc::new(bcc_handover.bcc()).map_err(|e| {
        error!("{e}");
        RebootReason::InvalidBcc
    })?;

    // The bootloader should never pass us a debug policy when the boot is secure (the bootloader
    // is locked). If it gets it wrong, disregard it & log it, to avoid it causing problems.
    if debug_policy.is_some() && !bcc.is_debug_mode() {
        warn!("Ignoring debug policy, BCC does not indicate Debug mode");
        debug_policy = None;
    }

    let verified_boot_data = verify_payload(signed_kernel, ramdisk, PUBLIC_KEY).map_err(|e| {
        error!("Failed to verify the payload: {e}");
        RebootReason::PayloadVerificationError
    })?;
    let debuggable = verified_boot_data.debug_level != DebugLevel::None;
    if debuggable {
        info!("Successfully verified a debuggable payload.");
        info!("Please disregard any previous libavb ERROR about initrd_normal.");
    }

    let guest_page_size = verified_boot_data.page_size.unwrap_or(SIZE_4KB);
    // TODO(ptosi): Cache the (single?) granule once, in vmbase.
    let hyp_page_size = if let Some(mem_sharer) = get_mem_sharer() {
        Some(mem_sharer.granule().map_err(|e| {
            error!("Failed to get granule size: {e}");
            RebootReason::InternalError
        })?)
    } else {
        None
    };
    let _ =
        sanitize_device_tree(untrusted_fdt, vm_dtbo, vm_ref_dt, guest_page_size, hyp_page_size)?;
    let fdt = untrusted_fdt; // DT has now been sanitized.

    let next_bcc_size = guest_page_size;
    let next_bcc = heap::aligned_boxed_slice(next_bcc_size, guest_page_size).ok_or_else(|| {
        error!("Failed to allocate the next-stage BCC");
        RebootReason::InternalError
    })?;
    // By leaking the slice, its content will be left behind for the next stage.
    let next_bcc = Box::leak(next_bcc);

    let dice_inputs = PartialInputs::new(&verified_boot_data).map_err(|e| {
        error!("Failed to compute partial DICE inputs: {e:?}");
        RebootReason::InternalError
    })?;

    let instance_hash = salt_from_instance_id(fdt)?;
    let (new_instance, salt, defer_rollback_protection) = perform_rollback_protection(
        fdt,
        &verified_boot_data,
        &dice_inputs,
        bcc_handover.cdi_seal(),
        instance_hash,
    )?;
    trace!("Got salt for instance: {salt:x?}");

    let new_bcc_handover = if cfg!(dice_changes) {
        Cow::Borrowed(current_bcc_handover)
    } else {
        // It is possible that the DICE chain we were given is rooted in the UDS. We do not want to
        // give such a chain to the payload, or even the associated CDIs. So remove the
        // entire chain we were given and taint the CDIs. Note that the resulting CDIs are
        // still deterministically derived from those we received, so will vary iff they do.
        // TODO(b/280405545): Remove this post Android 14.
        let truncated_bcc_handover = bcc::truncate(bcc_handover).map_err(|e| {
            error!("{e}");
            RebootReason::InternalError
        })?;
        Cow::Owned(truncated_bcc_handover)
    };

    trace!("BCC leaf subject public key algorithm: {:?}", bcc.leaf_subject_pubkey().cose_alg);

    let dice_context = DiceContext {
        authority_algorithm: bcc.leaf_subject_pubkey().cose_alg.try_into().map_err(|e| {
            error!("{e}");
            RebootReason::InternalError
        })?,
        subject_algorithm: VM_KEY_ALGORITHM,
    };
    dice_inputs
        .write_next_bcc(
            new_bcc_handover.as_ref(),
            &salt,
            instance_hash,
            defer_rollback_protection,
            next_bcc,
            dice_context,
        )
        .map_err(|e| {
            error!("Failed to derive next-stage DICE secrets: {e:?}");
            RebootReason::SecretDerivationError
        })?;
    flush(next_bcc);

    let kaslr_seed = u64::from_ne_bytes(rand::random_array().map_err(|e| {
        error!("Failed to generated guest KASLR seed: {e}");
        RebootReason::InternalError
    })?);
    let strict_boot = true;
    modify_for_next_stage(
        fdt,
        next_bcc,
        new_instance,
        strict_boot,
        debug_policy,
        debuggable,
        kaslr_seed,
    )
    .map_err(|e| {
        error!("Failed to configure device tree: {e}");
        RebootReason::InternalError
    })?;

    info!("Starting payload...");
    Ok((next_bcc, debuggable))
}

// Get the "salt" which is one of the input for DICE derivation.
// This provides differentiation of secrets for different VM instances with same payloads.
fn salt_from_instance_id(fdt: &Fdt) -> Result<Option<Hidden>, RebootReason> {
    let Some(id) = read_instance_id(fdt).map_err(|e| {
        error!("Failed to get instance-id in DT: {e}");
        RebootReason::InvalidFdt
    })?
    else {
        return Ok(None);
    };
    let salt = Digester::sha512()
        .digest(&[&b"InstanceId:"[..], id].concat())
        .map_err(|e| {
            error!("Failed to get digest of instance-id: {e}");
            RebootReason::InternalError
        })?
        .try_into()
        .map_err(|_| RebootReason::InternalError)?;
    Ok(Some(salt))
}
