// Copyright 2021, The Android Open Source Project
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

//! Functions for running instances of `crosvm`.

use crate::aidl::VirtualMachineCallbacks;
use crate::Cid;
use anyhow::{bail, Error};
use command_fds::CommandFdExt;
use log::{debug, error, info};
use shared_child::SharedChild;
use std::fs::{remove_dir_all, File};
use std::mem;
use std::num::NonZeroU32;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use vsock::VsockStream;

const CROSVM_PATH: &str = "/apex/com.android.virt/bin/crosvm";

/// Configuration for a VM to run with crosvm.
#[derive(Debug)]
pub struct CrosvmConfig {
    pub cid: Cid,
    pub bootloader: Option<File>,
    pub kernel: Option<File>,
    pub initrd: Option<File>,
    pub disks: Vec<DiskFile>,
    pub params: Option<String>,
    pub protected: bool,
    pub memory_mib: Option<NonZeroU32>,
    pub log_fd: Option<File>,
    pub indirect_files: Vec<File>,
}

/// A disk image to pass to crosvm for a VM.
#[derive(Debug)]
pub struct DiskFile {
    pub image: File,
    pub writable: bool,
}

/// The lifecycle state which the payload in the VM has reported itself to be in.
///
/// Note that the order of enum variants is significant; only forward transitions are allowed by
/// [`VmInstance::update_payload_state`].
#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PayloadState {
    Starting,
    Started,
    Ready,
    Finished,
}

/// The current state of the VM itself.
#[derive(Debug)]
pub enum VmState {
    /// The VM has not yet tried to start.
    NotStarted {
        ///The configuration needed to start the VM, if it has not yet been started.
        config: CrosvmConfig,
    },
    /// The VM has been started.
    Running {
        /// The crosvm child process.
        child: Arc<SharedChild>,
    },
    /// The VM died or was killed.
    Dead,
    /// The VM failed to start.
    Failed,
}

impl VmState {
    /// Tries to start the VM, if it is in the `NotStarted` state.
    ///
    /// Returns an error if the VM is in the wrong state, or fails to start.
    fn start(&mut self, instance: Arc<VmInstance>) -> Result<(), Error> {
        let state = mem::replace(self, VmState::Failed);
        if let VmState::NotStarted { config } = state {
            // If this fails and returns an error, `self` will be left in the `Failed` state.
            let child = Arc::new(run_vm(config)?);

            let child_clone = child.clone();
            thread::spawn(move || {
                instance.monitor(child_clone);
            });

            // If it started correctly, update the state.
            *self = VmState::Running { child };
            Ok(())
        } else {
            *self = state;
            bail!("VM already started or failed")
        }
    }
}

/// Information about a particular instance of a VM which may be running.
#[derive(Debug)]
pub struct VmInstance {
    /// The current state of the VM.
    pub vm_state: Mutex<VmState>,
    /// The CID assigned to the VM for vsock communication.
    pub cid: Cid,
    /// Whether the VM is a protected VM.
    pub protected: bool,
    /// Directory of temporary files used by the VM while it is running.
    pub temporary_directory: PathBuf,
    /// The UID of the process which requested the VM.
    pub requester_uid: u32,
    /// The SID of the process which requested the VM.
    pub requester_sid: String,
    /// The PID of the process which requested the VM. Note that this process may no longer exist
    /// and the PID may have been reused for a different process, so this should not be trusted.
    pub requester_debug_pid: i32,
    /// Callbacks to clients of the VM.
    pub callbacks: VirtualMachineCallbacks,
    /// Input/output stream of the payload run in the VM.
    pub stream: Mutex<Option<VsockStream>>,
    /// The latest lifecycle state which the payload reported itself to be in.
    payload_state: Mutex<PayloadState>,
}

impl VmInstance {
    /// Validates the given config and creates a new `VmInstance` but doesn't start running it.
    pub fn new(
        config: CrosvmConfig,
        temporary_directory: PathBuf,
        requester_uid: u32,
        requester_sid: String,
        requester_debug_pid: i32,
    ) -> Result<VmInstance, Error> {
        validate_config(&config)?;
        let cid = config.cid;
        let protected = config.protected;
        Ok(VmInstance {
            vm_state: Mutex::new(VmState::NotStarted { config }),
            cid,
            protected,
            temporary_directory,
            requester_uid,
            requester_sid,
            requester_debug_pid,
            callbacks: Default::default(),
            stream: Mutex::new(None),
            payload_state: Mutex::new(PayloadState::Starting),
        })
    }

    /// Starts an instance of `crosvm` to manage the VM. The `crosvm` instance will be killed when
    /// the `VmInstance` is dropped.
    pub fn start(self: &Arc<Self>) -> Result<(), Error> {
        self.vm_state.lock().unwrap().start(self.clone())
    }

    /// Waits for the crosvm child process to finish, then marks the VM as no longer running and
    /// calls any callbacks.
    ///
    /// This takes a separate reference to the `SharedChild` rather than using the one in
    /// `self.vm_state` to avoid holding the lock on `vm_state` while it is running.
    fn monitor(&self, child: Arc<SharedChild>) {
        match child.wait() {
            Err(e) => error!("Error waiting for crosvm instance to die: {}", e),
            Ok(status) => info!("crosvm exited with status {}", status),
        }

        let mut vm_state = self.vm_state.lock().unwrap();
        *vm_state = VmState::Dead;
        // Ensure that the mutex is released before calling the callbacks.
        drop(vm_state);

        self.callbacks.callback_on_died(self.cid);

        // Delete temporary files.
        if let Err(e) = remove_dir_all(&self.temporary_directory) {
            error!("Error removing temporary directory {:?}: {}", self.temporary_directory, e);
        }
    }

    /// Returns the last reported state of the VM payload.
    pub fn payload_state(&self) -> PayloadState {
        *self.payload_state.lock().unwrap()
    }

    /// Updates the payload state to the given value, if it is a valid state transition.
    pub fn update_payload_state(&self, new_state: PayloadState) -> Result<(), Error> {
        let mut state_locked = self.payload_state.lock().unwrap();
        // Only allow forward transitions, e.g. from starting to started or finished, not back in
        // the other direction.
        if new_state > *state_locked {
            *state_locked = new_state;
            Ok(())
        } else {
            bail!("Invalid payload state transition from {:?} to {:?}", *state_locked, new_state)
        }
    }

    /// Kills the crosvm instance, if it is running.
    pub fn kill(&self) {
        let vm_state = &*self.vm_state.lock().unwrap();
        if let VmState::Running { child } = vm_state {
            // TODO: Talk to crosvm to shutdown cleanly.
            if let Err(e) = child.kill() {
                error!("Error killing crosvm instance: {}", e);
            }
        }
    }
}

/// Starts an instance of `crosvm` to manage a new VM.
fn run_vm(config: CrosvmConfig) -> Result<SharedChild, Error> {
    validate_config(&config)?;

    let mut command = Command::new(CROSVM_PATH);
    // TODO(qwandor): Remove --disable-sandbox.
    command.arg("run").arg("--disable-sandbox").arg("--cid").arg(config.cid.to_string());

    if config.protected {
        command.arg("--protected-vm");
    }

    if let Some(memory_mib) = config.memory_mib {
        command.arg("--mem").arg(memory_mib.to_string());
    }

    if let Some(log_fd) = config.log_fd {
        command.stdout(log_fd);
    } else {
        // Ignore console output.
        command.arg("--serial=type=sink");
    }

    // Keep track of what file descriptors should be mapped to the crosvm process.
    let mut preserved_fds = config.indirect_files.iter().map(|file| file.as_raw_fd()).collect();

    if let Some(bootloader) = &config.bootloader {
        command.arg("--bios").arg(add_preserved_fd(&mut preserved_fds, bootloader));
    }

    if let Some(initrd) = &config.initrd {
        command.arg("--initrd").arg(add_preserved_fd(&mut preserved_fds, initrd));
    }

    if let Some(params) = &config.params {
        command.arg("--params").arg(params);
    }

    for disk in &config.disks {
        command
            .arg(if disk.writable { "--rwdisk" } else { "--disk" })
            .arg(add_preserved_fd(&mut preserved_fds, &disk.image));
    }

    if let Some(kernel) = &config.kernel {
        command.arg(add_preserved_fd(&mut preserved_fds, kernel));
    }

    debug!("Preserving FDs {:?}", preserved_fds);
    command.preserved_fds(preserved_fds);

    info!("Running {:?}", command);
    let result = SharedChild::spawn(&mut command)?;
    Ok(result)
}

/// Ensure that the configuration has a valid combination of fields set, or return an error if not.
fn validate_config(config: &CrosvmConfig) -> Result<(), Error> {
    if config.bootloader.is_none() && config.kernel.is_none() {
        bail!("VM must have either a bootloader or a kernel image.");
    }
    if config.bootloader.is_some() && (config.kernel.is_some() || config.initrd.is_some()) {
        bail!("Can't have both bootloader and kernel/initrd image.");
    }
    Ok(())
}

/// Adds the file descriptor for `file` to `preserved_fds`, and returns a string of the form
/// "/proc/self/fd/N" where N is the file descriptor.
fn add_preserved_fd(preserved_fds: &mut Vec<RawFd>, file: &File) -> String {
    let fd = file.as_raw_fd();
    preserved_fds.push(fd);
    format!("/proc/self/fd/{}", fd)
}
