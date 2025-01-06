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

//! High-level FDT functions.

pub mod pci;

use core::ops::Range;
use libfdt::{self, Fdt, FdtError};

/// Represents information about a SWIOTLB buffer.
#[derive(Debug)]
pub struct SwiotlbInfo {
    /// The address of the SWIOTLB buffer, if available.
    pub addr: Option<usize>,
    /// The size of the SWIOTLB buffer.
    pub size: usize,
    /// The alignment of the SWIOTLB buffer, if available.
    pub align: Option<usize>,
}

impl SwiotlbInfo {
    /// Creates a `SwiotlbInfo` struct from the given device tree.
    pub fn new_from_fdt(fdt: &Fdt) -> libfdt::Result<Option<SwiotlbInfo>> {
        let Some(node) = fdt.compatible_nodes(c"restricted-dma-pool")?.next() else {
            return Ok(None);
        };
        let (addr, size, align) = if let Some(mut reg) = node.reg()? {
            let reg = reg.next().ok_or(FdtError::BadValue)?;
            let size = reg.size.ok_or(FdtError::BadValue)?;
            (Some(reg.addr.try_into().unwrap()), size.try_into().unwrap(), None)
        } else {
            let size = node.getprop_u64(c"size")?.ok_or(FdtError::NotFound)?;
            let align = node.getprop_u64(c"alignment")?.ok_or(FdtError::NotFound)?;
            (None, size.try_into().unwrap(), Some(align.try_into().unwrap()))
        };
        Ok(Some(Self { addr, size, align }))
    }

    /// Returns the fixed range of memory mapped by the SWIOTLB buffer, if available.
    pub fn fixed_range(&self) -> Option<Range<usize>> {
        self.addr.map(|addr| addr..addr + self.size)
    }
}
