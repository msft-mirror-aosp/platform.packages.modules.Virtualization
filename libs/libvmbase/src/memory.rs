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

//! Memory management.

mod error;
mod shared;
mod stack;
mod tracker;
mod util;

pub use error::MemoryTrackerError;
pub use shared::MemoryRange;
pub use tracker::{
    deactivate_dynamic_page_tables, init_shared_pool, map_data, map_data_noflush, map_device,
    map_image_footer, map_rodata, map_rodata_outside_main_memory, resize_available_memory,
    unshare_all_memory, unshare_all_mmio_except_uart, unshare_uart,
};

#[cfg(target_arch = "aarch64")]
pub use crate::arch::aarch64::page_table::PageTable;

pub use util::{
    flush, flushed_zeroize, page_4kb_of, PAGE_SIZE, SIZE_128KB, SIZE_16KB, SIZE_2MB, SIZE_4KB,
    SIZE_4MB, SIZE_64KB,
};

pub(crate) use shared::{alloc_shared, dealloc_shared};
pub(crate) use stack::max_stack_size;
pub(crate) use tracker::{switch_to_dynamic_page_tables, MEMORY};
pub(crate) use util::{phys_to_virt, virt_to_phys};
