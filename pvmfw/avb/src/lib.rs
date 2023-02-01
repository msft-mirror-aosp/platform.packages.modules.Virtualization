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

//! A library implementing the payload verification for pvmfw with libavb

#![cfg_attr(not(test), no_std)]
// For usize.checked_add_signed(isize), available in Rust 1.66.0
#![feature(mixed_integer_ops)]

mod descriptor;
mod error;
mod ops;
mod partition;
mod utils;
mod verify;

pub use descriptor::Digest;
pub use error::AvbSlotVerifyError;
pub use verify::{verify_payload, DebugLevel, VerifiedBootData};
