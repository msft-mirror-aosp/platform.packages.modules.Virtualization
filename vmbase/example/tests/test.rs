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

//! Integration test for VM bootloader.

use android_system_virtualizationservice::{
    aidl::android::system::virtualizationservice::{
        DiskImage::DiskImage, VirtualMachineConfig::VirtualMachineConfig,
        VirtualMachineRawConfig::VirtualMachineRawConfig,
    },
    binder::{ParcelFileDescriptor, ProcessState},
};
use anyhow::{Context, Error};
use log::info;
use std::{
    fs::File,
    io::{self, BufRead, BufReader, Write},
    os::unix::io::FromRawFd,
    panic, thread,
};
use vmclient::{DeathReason, VmInstance};

const VMBASE_EXAMPLE_PATH: &str =
    "/data/local/tmp/vmbase_example.integration_test/arm64/vmbase_example.bin";
const TEST_DISK_IMAGE_PATH: &str = "/data/local/tmp/vmbase_example.integration_test/test_disk.img";

/// Runs the vmbase_example VM as an unprotected VM via VirtualizationService.
#[test]
fn test_run_example_vm() -> Result<(), Error> {
    android_logger::init_once(
        android_logger::Config::default().with_tag("vmbase").with_min_level(log::Level::Debug),
    );

    // Redirect panic messages to logcat.
    panic::set_hook(Box::new(|panic_info| {
        log::error!("{}", panic_info);
    }));

    // We need to start the thread pool for Binder to work properly, especially link_to_death.
    ProcessState::start_thread_pool();

    let virtmgr =
        vmclient::VirtualizationService::new().context("Failed to spawn VirtualizationService")?;
    let service = virtmgr.connect().context("Failed to connect to VirtualizationService")?;

    // Start example VM.
    let bootloader = ParcelFileDescriptor::new(
        File::open(VMBASE_EXAMPLE_PATH)
            .with_context(|| format!("Failed to open VM image {}", VMBASE_EXAMPLE_PATH))?,
    );

    // Make file for test disk image.
    let mut test_image = File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(TEST_DISK_IMAGE_PATH)
        .with_context(|| format!("Failed to open test disk image {}", TEST_DISK_IMAGE_PATH))?;
    // Write 4 sectors worth of 4-byte numbers counting up.
    for i in 0u32..512 {
        test_image.write_all(&i.to_le_bytes())?;
    }
    let test_image = ParcelFileDescriptor::new(test_image);
    let disk_image = DiskImage { image: Some(test_image), writable: false, partitions: vec![] };

    let config = VirtualMachineConfig::RawConfig(VirtualMachineRawConfig {
        name: String::from("VmBaseTest"),
        kernel: None,
        initrd: None,
        params: None,
        bootloader: Some(bootloader),
        disks: vec![disk_image],
        protectedVm: false,
        memoryMib: 300,
        numCpus: 1,
        platformVersion: "~1.0".to_string(),
        taskProfiles: vec![],
    });
    let console = android_log_fd()?;
    let log = android_log_fd()?;
    let vm = VmInstance::create(service.as_ref(), &config, Some(console), Some(log), None)
        .context("Failed to create VM")?;
    vm.start().context("Failed to start VM")?;
    info!("Started example VM.");

    // Wait for VM to finish, and check that it shut down cleanly.
    let death_reason = vm.wait_for_death();
    assert_eq!(death_reason, DeathReason::Shutdown);

    Ok(())
}

fn android_log_fd() -> io::Result<File> {
    let (reader_fd, writer_fd) = nix::unistd::pipe()?;

    // SAFETY: These are new FDs with no previous owner.
    let reader = unsafe { File::from_raw_fd(reader_fd) };
    let writer = unsafe { File::from_raw_fd(writer_fd) };

    thread::spawn(|| {
        for line in BufReader::new(reader).lines() {
            info!("{}", line.unwrap());
        }
    });
    Ok(writer)
}
