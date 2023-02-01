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

//! Safe MMIO_GUARD support.

use crate::helpers;
use crate::hvc::{mmio_guard_enroll, mmio_guard_info, mmio_guard_map, mmio_guard_unmap};
use crate::smccc;
use core::{fmt, result};

#[derive(Debug, Clone)]
pub enum Error {
    /// Failed the necessary MMIO_GUARD_ENROLL call.
    EnrollFailed(smccc::Error),
    /// Failed to obtain the MMIO_GUARD granule size.
    InfoFailed(smccc::Error),
    /// Failed to MMIO_GUARD_MAP a page.
    MapFailed(smccc::Error),
    /// Failed to MMIO_GUARD_UNMAP a page.
    UnmapFailed(smccc::Error),
    /// The MMIO_GUARD granule used by the hypervisor is not supported.
    UnsupportedGranule(usize),
}

type Result<T> = result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::EnrollFailed(e) => write!(f, "Failed to enroll into MMIO_GUARD: {e}"),
            Self::InfoFailed(e) => write!(f, "Failed to get the MMIO_GUARD granule: {e}"),
            Self::MapFailed(e) => write!(f, "Failed to MMIO_GUARD map: {e}"),
            Self::UnmapFailed(e) => write!(f, "Failed to MMIO_GUARD unmap: {e}"),
            Self::UnsupportedGranule(g) => write!(f, "Unsupported MMIO_GUARD granule: {g}"),
        }
    }
}

pub fn init() -> Result<()> {
    mmio_guard_enroll().map_err(Error::EnrollFailed)?;
    let mmio_granule = mmio_guard_info().map_err(Error::InfoFailed)? as usize;
    if mmio_granule != helpers::SIZE_4KB {
        return Err(Error::UnsupportedGranule(mmio_granule));
    }
    Ok(())
}

pub fn map(addr: usize) -> Result<()> {
    mmio_guard_map(helpers::page_4kb_of(addr) as u64).map_err(Error::MapFailed)
}

pub fn unmap(addr: usize) -> Result<()> {
    mmio_guard_unmap(helpers::page_4kb_of(addr) as u64).map_err(Error::UnmapFailed)
}
