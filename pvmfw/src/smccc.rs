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

use core::{fmt, result};

// TODO(b/245889995): use psci-0.1.1 crate
#[inline(always)]
pub fn hvc64(function: u32, args: [u64; 17]) -> [u64; 18] {
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let mut ret = [0; 18];

        core::arch::asm!(
            "hvc #0",
            inout("x0") function as u64 => ret[0],
            inout("x1") args[0] => ret[1],
            inout("x2") args[1] => ret[2],
            inout("x3") args[2] => ret[3],
            inout("x4") args[3] => ret[4],
            inout("x5") args[4] => ret[5],
            inout("x6") args[5] => ret[6],
            inout("x7") args[6] => ret[7],
            inout("x8") args[7] => ret[8],
            inout("x9") args[8] => ret[9],
            inout("x10") args[9] => ret[10],
            inout("x11") args[10] => ret[11],
            inout("x12") args[11] => ret[12],
            inout("x13") args[12] => ret[13],
            inout("x14") args[13] => ret[14],
            inout("x15") args[14] => ret[15],
            inout("x16") args[15] => ret[16],
            inout("x17") args[16] => ret[17],
            options(nomem, nostack)
        );

        ret
    }
}

/// Standard SMCCC error values as described in DEN 0028E.
#[derive(Debug, Clone)]
pub enum Error {
    /// The call is not supported by the implementation.
    NotSupported,
    /// The call is deemed not required by the implementation.
    NotRequired,
    /// One of the call parameters has a non-supported value.
    InvalidParameter,
    /// Negative values indicate error.
    Unknown(i64),
    /// The call returned a positive value when 0 was expected.
    Unexpected(u64),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::NotSupported => write!(f, "SMCCC call not supported"),
            Self::NotRequired => write!(f, "SMCCC call not required"),
            Self::InvalidParameter => write!(f, "SMCCC call received non-supported value"),
            Self::Unexpected(v) => write!(f, "Unexpected SMCCC return value {} ({0:#x})", v),
            Self::Unknown(e) => write!(f, "Unknown SMCCC return value {} ({0:#x})", e),
        }
    }
}

pub type Result<T> = result::Result<T, Error>;

pub fn checked_hvc64_expect_zero(function: u32, args: [u64; 17]) -> Result<()> {
    match checked_hvc64(function, args)? {
        0 => Ok(()),
        v => Err(Error::Unexpected(v)),
    }
}

pub fn checked_hvc64(function: u32, args: [u64; 17]) -> Result<u64> {
    match hvc64(function, args)[0] as i64 {
        ret if ret >= 0 => Ok(ret as u64),
        -1 => Err(Error::NotSupported),
        -2 => Err(Error::NotRequired),
        -3 => Err(Error::InvalidParameter),
        ret => Err(Error::Unknown(ret)),
    }
}
