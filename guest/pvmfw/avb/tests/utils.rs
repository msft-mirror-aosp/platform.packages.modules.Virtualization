/*
 * Copyright (C) 2023 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! Utility functions used by API tests.

use anyhow::{anyhow, Result};
use avb_bindgen::{
    avb_footer_validate_and_byteswap, avb_vbmeta_image_header_to_host_byte_order, AvbFooter,
    AvbVBMetaImageHeader,
};
use openssl::sha;
use pvmfw_avb::{
    verify_payload, Capability, DebugLevel, Digest, PvmfwVerifyError, VerifiedBootData,
};
use std::{
    fs,
    mem::{size_of, transmute, MaybeUninit},
};

const MICRODROID_KERNEL_IMG_PATH: &str = "microdroid_kernel";
const INITRD_NORMAL_IMG_PATH: &str = "microdroid_initrd_normal.img";
const INITRD_DEBUG_IMG_PATH: &str = "microdroid_initrd_debuggable.img";
const TRUSTY_TEST_VM_KERNEL_IMG_PATH: &str = "trusty_test_vm_signed.bin";
const PUBLIC_KEY_RSA4096_PATH: &str = "data/testkey_rsa4096_pub.bin";

pub const PUBLIC_KEY_RSA2048_PATH: &str = "data/testkey_rsa2048_pub.bin";

pub fn assert_payload_verification_with_initrd_fails(
    kernel: &[u8],
    initrd: &[u8],
    trusted_public_key: &[u8],
    expected_error: PvmfwVerifyError,
) -> Result<()> {
    assert_payload_verification_fails(kernel, Some(initrd), trusted_public_key, expected_error)
}

pub fn assert_payload_verification_fails(
    kernel: &[u8],
    initrd: Option<&[u8]>,
    trusted_public_key: &[u8],
    expected_error: PvmfwVerifyError,
) -> Result<()> {
    assert_eq!(expected_error, verify_payload(kernel, initrd, trusted_public_key).unwrap_err());
    Ok(())
}

pub fn load_latest_signed_kernel() -> Result<Vec<u8>> {
    Ok(fs::read(MICRODROID_KERNEL_IMG_PATH)?)
}

pub fn load_latest_trusty_test_vm_signed_kernel() -> Result<Vec<u8>> {
    Ok(fs::read(TRUSTY_TEST_VM_KERNEL_IMG_PATH)?)
}

pub fn load_latest_initrd_normal() -> Result<Vec<u8>> {
    Ok(fs::read(INITRD_NORMAL_IMG_PATH)?)
}

pub fn load_latest_initrd_debug() -> Result<Vec<u8>> {
    Ok(fs::read(INITRD_DEBUG_IMG_PATH)?)
}

pub fn load_trusted_public_key() -> Result<Vec<u8>> {
    Ok(fs::read(PUBLIC_KEY_RSA4096_PATH)?)
}

pub fn get_avb_footer_offset(signed_kernel: &[u8]) -> Result<usize> {
    let offset = signed_kernel.len().checked_sub(size_of::<AvbFooter>());

    offset.ok_or_else(|| anyhow!("Kernel too small to be AVB-signed"))
}

pub fn extract_avb_footer(kernel: &[u8]) -> Result<AvbFooter> {
    let footer_start = get_avb_footer_offset(kernel)?;
    // SAFETY: The slice is the same size as the struct which only contains simple data types.
    let mut footer = unsafe {
        transmute::<[u8; size_of::<AvbFooter>()], AvbFooter>(kernel[footer_start..].try_into()?)
    };
    // SAFETY: The function updates the struct in-place.
    unsafe {
        avb_footer_validate_and_byteswap(&footer, &mut footer);
    }
    Ok(footer)
}

pub fn extract_vbmeta_header(kernel: &[u8], footer: &AvbFooter) -> Result<AvbVBMetaImageHeader> {
    let vbmeta_offset: usize = footer.vbmeta_offset.try_into()?;
    let vbmeta_size: usize = footer.vbmeta_size.try_into()?;
    let vbmeta_src = &kernel[vbmeta_offset..(vbmeta_offset + vbmeta_size)];
    // SAFETY: The latest kernel has a valid VBMeta header at the position specified in footer.
    let vbmeta_header = unsafe {
        let mut header = MaybeUninit::uninit();
        let src = vbmeta_src.as_ptr() as *const _ as *const AvbVBMetaImageHeader;
        avb_vbmeta_image_header_to_host_byte_order(src, header.as_mut_ptr());
        header.assume_init()
    };
    Ok(vbmeta_header)
}

pub fn assert_latest_payload_verification_passes(
    initrd: &[u8],
    initrd_salt: &[u8],
    expected_debug_level: DebugLevel,
    page_size: Option<usize>,
) -> Result<()> {
    let public_key = load_trusted_public_key()?;
    let kernel = load_latest_signed_kernel()?;
    let verified_boot_data = verify_payload(&kernel, Some(initrd), &public_key)
        .map_err(|e| anyhow!("Verification failed. Error: {}", e))?;

    let footer = extract_avb_footer(&kernel)?;
    let kernel_digest =
        hash(&[&hash(&[b"bootloader"]), &kernel[..usize::try_from(footer.original_image_size)?]]);
    let capabilities = vec![Capability::SecretkeeperProtection];
    let initrd_digest = Some(hash(&[&hash(&[initrd_salt]), initrd]));
    let expected_boot_data = VerifiedBootData {
        debug_level: expected_debug_level,
        kernel_digest,
        initrd_digest,
        public_key: &public_key,
        capabilities,
        rollback_index: 1,
        page_size,
    };
    assert_eq!(expected_boot_data, verified_boot_data);

    Ok(())
}

pub fn assert_payload_without_initrd_passes_verification(
    kernel: &[u8],
    salt: &[u8],
    expected_rollback_index: u64,
    capabilities: Vec<Capability>,
    page_size: Option<usize>,
) -> Result<()> {
    let public_key = load_trusted_public_key()?;
    let verified_boot_data = verify_payload(
        kernel,
        None, // initrd
        &public_key,
    )
    .map_err(|e| anyhow!("Verification failed. Error: {}", e))?;

    let footer = extract_avb_footer(kernel)?;
    let kernel_digest =
        hash(&[&hash(&[salt]), &kernel[..usize::try_from(footer.original_image_size)?]]);
    let expected_boot_data = VerifiedBootData {
        debug_level: DebugLevel::None,
        kernel_digest,
        initrd_digest: None,
        public_key: &public_key,
        capabilities,
        rollback_index: expected_rollback_index,
        page_size,
    };
    assert_eq!(expected_boot_data, verified_boot_data);

    Ok(())
}

pub fn read_page_size(kernel: &[u8]) -> Result<Option<usize>, PvmfwVerifyError> {
    let public_key = load_trusted_public_key().unwrap();
    let verified_boot_data = verify_payload(
        kernel,
        None, // initrd
        &public_key,
    )?;
    Ok(verified_boot_data.page_size)
}

pub fn hash(inputs: &[&[u8]]) -> Digest {
    let mut digester = sha::Sha256::new();
    inputs.iter().for_each(|input| digester.update(input));
    digester.finish()
}
