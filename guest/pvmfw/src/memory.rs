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

//! Low-level allocation and tracking of main memory.

use crate::entry::RebootReason;
use crate::fdt;
use aarch64_paging::MapError;
use core::num::NonZeroUsize;
use core::result;
use core::slice;
use log::debug;
use log::error;
use log::info;
use log::warn;
use vmbase::{
    layout::{self, crosvm},
    memory::{init_shared_pool, map_data, map_rodata, resize_available_memory, PageTable},
};

pub fn init_page_table() -> result::Result<PageTable, MapError> {
    let mut page_table = PageTable::default();

    // Stack and scratch ranges are explicitly zeroed and flushed before jumping to payload,
    // so dirty state management can be omitted.
    page_table.map_data(&layout::data_bss_range().into())?;
    page_table.map_data(&layout::eh_stack_range().into())?;
    page_table.map_data(&layout::stack_range().into())?;
    page_table.map_code(&layout::text_range().into())?;
    page_table.map_rodata(&layout::rodata_range().into())?;
    if let Err(e) = page_table.map_device(&layout::console_uart_page().into()) {
        error!("Failed to remap the UART as a dynamic page table entry: {e}");
        return Err(e);
    }
    Ok(page_table)
}

pub(crate) struct MemorySlices<'a> {
    pub fdt: &'a mut libfdt::Fdt,
    pub kernel: &'a [u8],
    pub ramdisk: Option<&'a [u8]>,
}

impl<'a> MemorySlices<'a> {
    pub fn new(
        fdt: usize,
        kernel: usize,
        kernel_size: usize,
        vm_dtbo: Option<&mut [u8]>,
        vm_ref_dt: Option<&[u8]>,
    ) -> Result<Self, RebootReason> {
        let fdt_size = NonZeroUsize::new(crosvm::FDT_MAX_SIZE).unwrap();
        // TODO - Only map the FDT as read-only, until we modify it right before jump_to_payload()
        // e.g. by generating a DTBO for a template DT in main() and, on return, re-map DT as RW,
        // overwrite with the template DT and apply the DTBO.
        map_data(fdt, fdt_size).map_err(|e| {
            error!("Failed to allocate the FDT range: {e}");
            RebootReason::InternalError
        })?;

        // SAFETY: map_data validated the range to be in main memory, mapped, and not overlap.
        let fdt = unsafe { slice::from_raw_parts_mut(fdt as *mut u8, fdt_size.into()) };

        let info = fdt::sanitize_device_tree(fdt, vm_dtbo, vm_ref_dt)?;
        let fdt = libfdt::Fdt::from_mut_slice(fdt).map_err(|e| {
            error!("Failed to load sanitized FDT: {e}");
            RebootReason::InvalidFdt
        })?;
        debug!("Fdt passed validation!");

        let memory_range = info.memory_range;
        debug!("Resizing MemoryTracker to range {memory_range:#x?}");
        resize_available_memory(&memory_range).map_err(|e| {
            error!("Failed to use memory range value from DT: {memory_range:#x?}: {e}");
            RebootReason::InvalidFdt
        })?;

        init_shared_pool(info.swiotlb_info.fixed_range()).map_err(|e| {
            error!("Failed to initialize shared pool: {e}");
            RebootReason::InternalError
        })?;

        let (kernel_start, kernel_size) = if let Some(r) = info.kernel_range {
            let size = r.len().try_into().map_err(|_| {
                error!("Invalid kernel size: {:#x}", r.len());
                RebootReason::InternalError
            })?;
            (r.start, size)
        } else if cfg!(feature = "legacy") {
            warn!("Failed to find the kernel range in the DT; falling back to legacy ABI");
            let size = NonZeroUsize::new(kernel_size).ok_or_else(|| {
                error!("Invalid kernel size: {kernel_size:#x}");
                RebootReason::InvalidPayload
            })?;
            (kernel, size)
        } else {
            error!("Failed to locate the kernel from the DT");
            return Err(RebootReason::InvalidPayload);
        };

        map_rodata(kernel_start, kernel_size).map_err(|e| {
            error!("Failed to map kernel range: {e}");
            RebootReason::InternalError
        })?;

        let kernel = kernel_start as *const u8;
        // SAFETY: map_rodata validated the range to be in main memory, mapped, and not overlap.
        let kernel = unsafe { slice::from_raw_parts(kernel, kernel_size.into()) };

        let ramdisk = if let Some(r) = info.initrd_range {
            debug!("Located ramdisk at {r:?}");
            let ramdisk_size = r.len().try_into().map_err(|_| {
                error!("Invalid ramdisk size: {:#x}", r.len());
                RebootReason::InvalidRamdisk
            })?;
            map_rodata(r.start, ramdisk_size).map_err(|e| {
                error!("Failed to obtain the initrd range: {e}");
                RebootReason::InvalidRamdisk
            })?;

            // SAFETY: map_rodata validated the range to be in main memory, mapped, and not
            // overlap.
            Some(unsafe { slice::from_raw_parts(r.start as *const u8, r.len()) })
        } else {
            info!("Couldn't locate the ramdisk from the device tree");
            None
        };

        Ok(Self { fdt, kernel, ramdisk })
    }
}
