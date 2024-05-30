// Copyright 2024, The Android Open Source Project
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

//! Implementation of the AIDL interface of Vmnic.

use anyhow::{anyhow, Context, Result};
use android_system_virtualizationservice_internal::aidl::android::system::virtualizationservice_internal::IVmnic::IVmnic;
use binder::{self, Interface, IntoBinderResult, ParcelFileDescriptor};
use libc::{c_char, c_int, c_short, ifreq, IFF_NO_PI, IFF_TAP, IFF_UP, IFNAMSIZ};
use log::info;
use nix::{ioctl_write_int_bad, ioctl_write_ptr_bad};
use nix::sys::ioctl::ioctl_num_type;
use nix::sys::socket::{socket, AddressFamily, SockFlag, SockType};
use std::ffi::CString;
use std::fs::File;
use std::os::fd::{AsRawFd, RawFd};
use std::slice::from_raw_parts;

const TUNSETIFF: ioctl_num_type = 0x400454ca;
const TUNSETPERSIST: ioctl_num_type = 0x400454cb;
const SIOCGIFFLAGS: ioctl_num_type = 0x00008913;
const SIOCSIFFLAGS: ioctl_num_type = 0x00008914;

ioctl_write_ptr_bad!(ioctl_tunsetiff, TUNSETIFF, ifreq);
ioctl_write_int_bad!(ioctl_tunsetpersist, TUNSETPERSIST);
ioctl_write_ptr_bad!(ioctl_siocgifflags, SIOCGIFFLAGS, ifreq);
ioctl_write_ptr_bad!(ioctl_siocsifflags, SIOCSIFFLAGS, ifreq);

fn validate_ifname(ifname: &[c_char]) -> Result<()> {
    if ifname.len() >= IFNAMSIZ {
        return Err(anyhow!(format!("Interface name is too long")));
    }
    Ok(())
}

fn create_tap_interface(fd: RawFd, ifname: &[c_char]) -> Result<()> {
    // SAFETY: All-zero is a valid value for the ifreq type.
    let mut ifr: ifreq = unsafe { std::mem::zeroed() };
    ifr.ifr_ifru.ifru_flags = (IFF_TAP | IFF_NO_PI) as c_short;
    ifr.ifr_name[..ifname.len()].copy_from_slice(ifname);
    // SAFETY: `ioctl` is copied into the kernel. It modifies the state in the kernel, not the
    // state of this process in any way.
    unsafe { ioctl_tunsetiff(fd, &ifr) }.context("Failed to ioctl TUNSETIFF")?;
    // SAFETY: `ioctl` is copied into the kernel. It modifies the state in the kernel, not the
    // state of this process in any way.
    unsafe { ioctl_tunsetpersist(fd, 1) }.context("Failed to ioctl TUNSETPERSIST")?;
    Ok(())
}

fn bring_up_interface(sockfd: c_int, ifname: &[c_char]) -> Result<()> {
    // SAFETY: All-zero is a valid value for the ifreq type.
    let mut ifr: ifreq = unsafe { std::mem::zeroed() };
    ifr.ifr_name[..ifname.len()].copy_from_slice(ifname);
    // SAFETY: `ioctl` is copied into the kernel. It modifies the state in the kernel, not the
    // state of this process in any way.
    unsafe { ioctl_siocgifflags(sockfd, &ifr) }.context("Failed to ioctl SIOCGIFFLAGS")?;
    // SAFETY: After calling SIOCGIFFLAGS, ifr_ifru holds ifru_flags in its union field.
    unsafe { ifr.ifr_ifru.ifru_flags |= IFF_UP as c_short };
    // SAFETY: `ioctl` is copied into the kernel. It modifies the state in the kernel, not the
    // state of this process in any way.
    unsafe { ioctl_siocsifflags(sockfd, &ifr) }.context("Failed to ioctl SIOCGIFFLAGS")?;
    Ok(())
}

#[derive(Debug, Default)]
pub struct Vmnic {}

impl Vmnic {
    pub fn init() -> Vmnic {
        Vmnic::default()
    }
}

impl Interface for Vmnic {}

impl IVmnic for Vmnic {
    fn createTapInterface(&self, iface_name_suffix: &str) -> binder::Result<ParcelFileDescriptor> {
        let ifname = CString::new(format!("avf_tap_{iface_name_suffix}"))
            .context(format!(
                "Failed to construct TAP interface name as CString: avf_tap_{iface_name_suffix}"
            ))
            .or_service_specific_exception(-1)?;
        let ifname_bytes = ifname.as_bytes_with_nul();
        // SAFETY: Converting from &[u8] into &[c_char].
        let ifname_bytes =
            unsafe { from_raw_parts(ifname_bytes.as_ptr().cast::<c_char>(), ifname_bytes.len()) };
        validate_ifname(ifname_bytes)
            .context(format!("Invalid interface name: {ifname:#?}"))
            .or_service_specific_exception(-1)?;

        let tunfd = File::open("/dev/tun")
            .context("Failed to open /dev/tun")
            .or_service_specific_exception(-1)?;
        create_tap_interface(tunfd.as_raw_fd(), ifname_bytes)
            .context(format!("Failed to create TAP interface: {ifname:#?}"))
            .or_service_specific_exception(-1)?;

        let sock = socket(AddressFamily::Inet, SockType::Datagram, SockFlag::empty(), None)
            .context("Failed to create socket")
            .or_service_specific_exception(-1)?;
        bring_up_interface(sock.as_raw_fd(), ifname_bytes)
            .context(format!("Failed to bring up TAP interface: {ifname:#?}"))
            .or_service_specific_exception(-1)?;

        info!("Created TAP network interface: {ifname:#?}");
        Ok(ParcelFileDescriptor::new(tunfd))
    }
}
