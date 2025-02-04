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

//! Memory layout.

#![allow(unused_unsafe)]

#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::linker::__stack_chk_guard;
use crate::arch::VirtualAddress;
use crate::memory::{max_stack_size, PAGE_SIZE};
use core::ops::Range;

#[cfg(target_arch = "aarch64")]
pub use crate::arch::aarch64::layout as crosvm;

/// First address that can't be translated by a level 1 TTBR0_EL1.
pub const MAX_VIRT_ADDR: usize = 1 << 40;

/// Get an address from a linker-defined symbol.
#[macro_export]
macro_rules! linker_addr {
    ($symbol:ident) => {{
        #[cfg(target_arch = "aarch64")]
        let addr = (&raw const $crate::arch::aarch64::linker::$symbol) as usize;
        VirtualAddress(addr)
    }};
}

/// Gets the virtual address range between a pair of linker-defined symbols.
#[macro_export]
macro_rules! linker_region {
    ($begin:ident,$end:ident) => {{
        let start = linker_addr!($begin);
        let end = linker_addr!($end);
        start..end
    }};
}

/// Executable code.
pub fn text_range() -> Range<VirtualAddress> {
    linker_region!(text_begin, text_end)
}

/// Read-only data.
pub fn rodata_range() -> Range<VirtualAddress> {
    linker_region!(rodata_begin, rodata_end)
}

/// Region which may contain a footer appended to the binary at load time.
pub fn image_footer_range() -> Range<VirtualAddress> {
    linker_region!(image_footer_begin, image_footer_end)
}

/// Initialised writable data.
pub fn data_range() -> Range<VirtualAddress> {
    linker_region!(data_begin, data_end)
}

/// Zero-initialized writable data.
pub fn bss_range() -> Range<VirtualAddress> {
    linker_region!(bss_begin, bss_end)
}

/// Writable data region for .data and .bss.
pub fn data_bss_range() -> Range<VirtualAddress> {
    linker_region!(data_begin, bss_end)
}

/// Writable data region for the stack.
pub fn stack_range() -> Range<VirtualAddress> {
    let end = linker_addr!(init_stack_pointer);
    let start = if let Some(stack_size) = max_stack_size() {
        assert_eq!(stack_size % PAGE_SIZE, 0);
        let start = VirtualAddress(end.0.checked_sub(stack_size).unwrap());
        assert!(start >= linker_addr!(stack_limit));
        start
    } else {
        linker_addr!(stack_limit)
    };

    start..end
}

/// Writable data region for the exception handler stack.
pub fn eh_stack_range() -> Range<VirtualAddress> {
    linker_region!(eh_stack_limit, init_eh_stack_pointer)
}

/// Range of the page at UART_PAGE_ADDR of PAGE_SIZE.
#[cfg(target_arch = "aarch64")]
pub fn console_uart_page() -> Range<VirtualAddress> {
    VirtualAddress(crosvm::UART_PAGE_ADDR)..VirtualAddress(crosvm::UART_PAGE_ADDR + PAGE_SIZE)
}

/// Read-write data (original).
pub fn data_load_address() -> VirtualAddress {
    linker_addr!(data_lma)
}

/// End of the binary image.
pub fn binary_end() -> VirtualAddress {
    linker_addr!(bin_end)
}

/// Value of __stack_chk_guard.
pub fn stack_chk_guard() -> u64 {
    // SAFETY: __stack_chk_guard shouldn't have any mutable aliases unless the stack overflows. If
    // it does, then there could be undefined behaviour all over the program, but we want to at
    // least have a chance at catching it.
    unsafe { (&raw const __stack_chk_guard).read_volatile() }
}
