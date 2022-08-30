/*
 * Copyright 2021 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! Handle running odrefresh in the VM, with an async interface to allow cancellation

use crate::fd_server_helper::FdServerConfig;
use crate::instance_starter::CompOsInstance;
use android_system_composd::aidl::android::system::composd::{
    ICompilationTask::ICompilationTask,
    ICompilationTaskCallback::{FailureReason::FailureReason, ICompilationTaskCallback},
};
use anyhow::{Context, Result};
use binder::{Interface, Result as BinderResult, Strong};
use compos_aidl_interface::aidl::com::android::compos::ICompOsService::{
    CompilationMode::CompilationMode, ICompOsService,
};
use compos_common::odrefresh::{
    is_system_property_interesting, ExitCode, ODREFRESH_OUTPUT_ROOT_DIR,
};
use log::{error, info, warn};
use rustutils::system_properties;
use std::fs::{remove_dir_all, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, OwnedFd};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone)]
pub struct OdrefreshTask {
    running_task: Arc<Mutex<Option<RunningTask>>>,
}

impl Interface for OdrefreshTask {}

impl ICompilationTask for OdrefreshTask {
    fn cancel(&self) -> BinderResult<()> {
        let task = self.take();
        // Drop the VM, which should end compilation - and cause our thread to exit.
        // Note that we don't do a graceful shutdown here; we've been asked to give up our resources
        // ASAP, and the VM has not failed so we don't need to ensure VM logs are written.
        drop(task);
        Ok(())
    }
}

struct RunningTask {
    callback: Strong<dyn ICompilationTaskCallback>,
    #[allow(dead_code)] // Keeps the CompOS VM alive
    comp_os: CompOsInstance,
}

impl OdrefreshTask {
    /// Return the current running task, if any, removing it from this CompilationTask.
    /// Once removed, meaning the task has ended or been canceled, further calls will always return
    /// None.
    fn take(&self) -> Option<RunningTask> {
        self.running_task.lock().unwrap().take()
    }

    pub fn start(
        comp_os: CompOsInstance,
        compilation_mode: CompilationMode,
        target_dir_name: String,
        callback: &Strong<dyn ICompilationTaskCallback>,
    ) -> Result<OdrefreshTask> {
        let service = comp_os.get_service();
        let task = RunningTask { comp_os, callback: callback.clone() };
        let task = OdrefreshTask { running_task: Arc::new(Mutex::new(Some(task))) };

        task.clone().start_thread(service, compilation_mode, target_dir_name);

        Ok(task)
    }

    fn start_thread(
        self,
        service: Strong<dyn ICompOsService>,
        compilation_mode: CompilationMode,
        target_dir_name: String,
    ) {
        thread::spawn(move || {
            let exit_code = run_in_vm(service, compilation_mode, &target_dir_name);

            let task = self.take();
            // We don't do the callback if cancel has already happened.
            if let Some(RunningTask { callback, comp_os }) = task {
                // Make sure we keep our service alive until we have called the callback.
                let lazy_service_guard = comp_os.shutdown();

                let result = match exit_code {
                    Ok(ExitCode::CompilationSuccess) => {
                        info!("CompilationSuccess");
                        callback.onSuccess()
                    }
                    Ok(exit_code) => {
                        let message = format!("Unexpected odrefresh result: {:?}", exit_code);
                        error!("{}", message);
                        callback.onFailure(FailureReason::UnexpectedCompilationResult, &message)
                    }
                    Err(e) => {
                        let message = format!("Running odrefresh failed: {:?}", e);
                        error!("{}", message);
                        callback.onFailure(FailureReason::CompilationFailed, &message)
                    }
                };
                if let Err(e) = result {
                    warn!("Failed to deliver callback: {:?}", e);
                }
                drop(lazy_service_guard);
            }
        });
    }
}

fn run_in_vm(
    service: Strong<dyn ICompOsService>,
    compilation_mode: CompilationMode,
    target_dir_name: &str,
) -> Result<ExitCode> {
    let mut names = Vec::new();
    let mut values = Vec::new();
    system_properties::foreach(|name, value| {
        if is_system_property_interesting(name) {
            names.push(name.to_owned());
            values.push(value.to_owned());
        }
    })?;
    service.initializeSystemProperties(&names, &values).context("initialize system properties")?;

    let output_root = Path::new(ODREFRESH_OUTPUT_ROOT_DIR);

    // We need to remove the target directory because odrefresh running in compos will create it
    // (and can't see the existing one, since authfs doesn't show it existing files in an output
    // directory).
    let target_path = output_root.join(target_dir_name);
    if target_path.exists() {
        remove_dir_all(&target_path)
            .with_context(|| format!("Failed to delete {}", target_path.display()))?;
    }

    let staging_dir_fd = open_dir(composd_native::palette_create_odrefresh_staging_directory()?)?;
    let system_dir_fd = open_dir(Path::new("/system"))?;
    let output_dir_fd = open_dir(output_root)?;

    // Get the raw FD before passing the ownership, since borrowing will violate the borrow check.
    let system_dir_raw_fd = system_dir_fd.as_raw_fd();
    let output_dir_raw_fd = output_dir_fd.as_raw_fd();
    let staging_dir_raw_fd = staging_dir_fd.as_raw_fd();

    // Spawn a fd_server to serve the FDs.
    let fd_server_config = FdServerConfig {
        ro_dir_fds: vec![system_dir_fd],
        rw_dir_fds: vec![staging_dir_fd, output_dir_fd],
        ..Default::default()
    };
    let fd_server_raii = fd_server_config.into_fd_server()?;

    let zygote_arch = system_properties::read("ro.zygote")?.context("ro.zygote not set")?;
    let system_server_compiler_filter =
        system_properties::read("dalvik.vm.systemservercompilerfilter")?.unwrap_or_default();
    let exit_code = service.odrefresh(
        compilation_mode,
        system_dir_raw_fd,
        output_dir_raw_fd,
        staging_dir_raw_fd,
        target_dir_name,
        &zygote_arch,
        &system_server_compiler_filter,
    )?;

    drop(fd_server_raii);
    ExitCode::from_i32(exit_code.into())
}

/// Returns an `OwnedFD` of the directory.
fn open_dir(path: &Path) -> Result<OwnedFd> {
    Ok(OwnedFd::from(
        OpenOptions::new()
            .custom_flags(libc::O_DIRECTORY)
            .read(true) // O_DIRECTORY can only be opened with read
            .open(path)
            .with_context(|| format!("Failed to open {:?} directory as path fd", path))?,
    ))
}
