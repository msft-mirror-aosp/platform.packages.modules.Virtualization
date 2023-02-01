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

//! Functions for creating and collecting atoms.

use crate::aidl::{clone_file, GLOBAL_SERVICE};
use crate::crosvm::VmMetric;
use crate::get_calling_uid;
use android_system_virtualizationcommon::aidl::android::system::virtualizationcommon::DeathReason::DeathReason;
use android_system_virtualizationservice::aidl::android::system::virtualizationservice::{
    IVirtualMachine::IVirtualMachine,
    VirtualMachineAppConfig::{Payload::Payload, VirtualMachineAppConfig},
    VirtualMachineConfig::VirtualMachineConfig,
};
use android_system_virtualizationservice::binder::{Status, Strong};
use android_system_virtualizationservice_internal::aidl::android::system::virtualizationservice_internal::{
    AtomVmBooted::AtomVmBooted,
    AtomVmCreationRequested::AtomVmCreationRequested,
    AtomVmExited::AtomVmExited,
};
use anyhow::{anyhow, Result};
use binder::ParcelFileDescriptor;
use log::warn;
use microdroid_payload_config::VmPayloadConfig;
use statslog_virtualization_rust::vm_creation_requested;
use std::thread;
use std::time::{Duration, SystemTime};
use zip::ZipArchive;

fn get_apex_list(config: &VirtualMachineAppConfig) -> String {
    match &config.payload {
        Payload::PayloadConfig(_) => String::new(),
        Payload::ConfigPath(config_path) => {
            let vm_payload_config = get_vm_payload_config(&config.apk, config_path);
            if let Ok(vm_payload_config) = vm_payload_config {
                vm_payload_config
                    .apexes
                    .iter()
                    .map(|x| x.name.clone())
                    .collect::<Vec<String>>()
                    .join(":")
            } else {
                "INFO: Can't get VmPayloadConfig".to_owned()
            }
        }
    }
}

fn get_vm_payload_config(
    apk_fd: &Option<ParcelFileDescriptor>,
    config_path: &str,
) -> Result<VmPayloadConfig> {
    let apk = apk_fd.as_ref().ok_or_else(|| anyhow!("APK is none"))?;
    let apk_file = clone_file(apk)?;
    let mut apk_zip = ZipArchive::new(&apk_file)?;
    let config_file = apk_zip.by_name(config_path)?;
    let vm_payload_config: VmPayloadConfig = serde_json::from_reader(config_file)?;
    Ok(vm_payload_config)
}

fn get_duration(vm_start_timestamp: Option<SystemTime>) -> Duration {
    match vm_start_timestamp {
        Some(vm_start_timestamp) => vm_start_timestamp.elapsed().unwrap_or_default(),
        None => Duration::default(),
    }
}

/// Write the stats of VMCreation to statsd
pub fn write_vm_creation_stats(
    config: &VirtualMachineConfig,
    is_protected: bool,
    ret: &binder::Result<Strong<dyn IVirtualMachine>>,
) {
    let creation_succeeded;
    let binder_exception_code;
    match ret {
        Ok(_) => {
            creation_succeeded = true;
            binder_exception_code = Status::ok().exception_code() as i32;
        }
        Err(ref e) => {
            creation_succeeded = false;
            binder_exception_code = e.exception_code() as i32;
        }
    }
    let (vm_identifier, config_type, num_cpus, memory_mib, apexes) = match config {
        VirtualMachineConfig::AppConfig(config) => (
            config.name.clone(),
            vm_creation_requested::ConfigType::VirtualMachineAppConfig,
            config.numCpus,
            config.memoryMib,
            get_apex_list(config),
        ),
        VirtualMachineConfig::RawConfig(config) => (
            config.name.clone(),
            vm_creation_requested::ConfigType::VirtualMachineRawConfig,
            config.numCpus,
            config.memoryMib,
            String::new(),
        ),
    };

    let atom = AtomVmCreationRequested {
        uid: get_calling_uid() as i32,
        vmIdentifier: vm_identifier,
        isProtected: is_protected,
        creationSucceeded: creation_succeeded,
        binderExceptionCode: binder_exception_code,
        configType: config_type as i32,
        numCpus: num_cpus,
        memoryMib: memory_mib,
        apexes,
    };

    thread::spawn(move || {
        GLOBAL_SERVICE.atomVmCreationRequested(&atom).unwrap_or_else(|e| {
            warn!("Failed to write VmCreationRequested atom: {e}");
        });
    });
}

/// Write the stats of VM boot to statsd
/// The function creates a separate thread which waits fro statsd to start to push atom
pub fn write_vm_booted_stats(
    uid: i32,
    vm_identifier: &str,
    vm_start_timestamp: Option<SystemTime>,
) {
    let vm_identifier = vm_identifier.to_owned();
    let duration = get_duration(vm_start_timestamp);

    let atom = AtomVmBooted {
        uid,
        vmIdentifier: vm_identifier,
        elapsedTimeMillis: duration.as_millis() as i64,
    };

    thread::spawn(move || {
        GLOBAL_SERVICE.atomVmBooted(&atom).unwrap_or_else(|e| {
            warn!("Failed to write VmCreationRequested atom: {e}");
        });
    });
}

/// Write the stats of VM exit to statsd
/// The function creates a separate thread which waits fro statsd to start to push atom
pub fn write_vm_exited_stats(
    uid: i32,
    vm_identifier: &str,
    reason: DeathReason,
    exit_signal: Option<i32>,
    vm_metric: &VmMetric,
) {
    let vm_identifier = vm_identifier.to_owned();
    let elapsed_time_millis = get_duration(vm_metric.start_timestamp).as_millis() as i64;
    let guest_time_millis = vm_metric.cpu_guest_time.unwrap_or_default();
    let rss = vm_metric.rss.unwrap_or_default();

    let atom = AtomVmExited {
        uid,
        vmIdentifier: vm_identifier,
        elapsedTimeMillis: elapsed_time_millis,
        deathReason: reason,
        guestTimeMillis: guest_time_millis,
        rssVmKb: rss.vm,
        rssCrosvmKb: rss.crosvm,
        exitSignal: exit_signal.unwrap_or_default(),
    };

    thread::spawn(move || {
        GLOBAL_SERVICE.atomVmExited(&atom).unwrap_or_else(|e| {
            warn!("Failed to write VmExited atom: {e}");
        });
    });
}
