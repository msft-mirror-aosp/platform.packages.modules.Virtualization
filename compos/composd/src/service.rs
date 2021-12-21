/*
 * Copyright (C) 2021 The Android Open Source Project
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

//! Implementation of IIsolatedCompilationService, called from system server when compilation is
//! desired.

use crate::instance_manager::InstanceManager;
use crate::odrefresh_task::OdrefreshTask;
use android_system_composd::aidl::android::system::composd::{
    ICompilationTask::{BnCompilationTask, ICompilationTask},
    ICompilationTaskCallback::ICompilationTaskCallback,
    IIsolatedCompilationService::{BnIsolatedCompilationService, IIsolatedCompilationService},
};
use android_system_composd::binder::{
    self, BinderFeatures, ExceptionCode, Interface, Status, Strong, ThreadState,
};
use anyhow::{Context, Result};
use compos_common::binder::to_binder_result;
use rustutils::{users::AID_ROOT, users::AID_SYSTEM};
use std::sync::Arc;

pub struct IsolatedCompilationService {
    instance_manager: Arc<InstanceManager>,
}

pub fn new_binder(
    instance_manager: Arc<InstanceManager>,
) -> Strong<dyn IIsolatedCompilationService> {
    let service = IsolatedCompilationService { instance_manager };
    BnIsolatedCompilationService::new_binder(service, BinderFeatures::default())
}

impl Interface for IsolatedCompilationService {}

impl IIsolatedCompilationService for IsolatedCompilationService {
    fn startStagedApexCompile(
        &self,
        callback: &Strong<dyn ICompilationTaskCallback>,
    ) -> binder::Result<Strong<dyn ICompilationTask>> {
        check_permissions()?;
        to_binder_result(self.do_start_staged_apex_compile(callback))
    }

    fn startTestCompile(
        &self,
        callback: &Strong<dyn ICompilationTaskCallback>,
    ) -> binder::Result<Strong<dyn ICompilationTask>> {
        check_permissions()?;
        to_binder_result(self.do_start_test_compile(callback))
    }
}

impl IsolatedCompilationService {
    fn do_start_staged_apex_compile(
        &self,
        callback: &Strong<dyn ICompilationTaskCallback>,
    ) -> Result<Strong<dyn ICompilationTask>> {
        // TODO: Try to start the current instance with staged APEXes to see if it works?
        let comp_os = self.instance_manager.start_pending_instance().context("Starting CompOS")?;

        // TODO: Write to compos-pending instead
        let target_dir_name = "test-artifacts".to_owned();
        let task = OdrefreshTask::start(comp_os, target_dir_name, callback)?;

        Ok(BnCompilationTask::new_binder(task, BinderFeatures::default()))
    }

    fn do_start_test_compile(
        &self,
        callback: &Strong<dyn ICompilationTaskCallback>,
    ) -> Result<Strong<dyn ICompilationTask>> {
        let comp_os = self.instance_manager.start_test_instance().context("Starting CompOS")?;

        let target_dir_name = "test-artifacts".to_owned();
        let task = OdrefreshTask::start(comp_os, target_dir_name, callback)?;

        Ok(BnCompilationTask::new_binder(task, BinderFeatures::default()))
    }
}

fn check_permissions() -> binder::Result<()> {
    let calling_uid = ThreadState::get_calling_uid();
    // This should only be called by system server, or root while testing
    if calling_uid != AID_SYSTEM && calling_uid != AID_ROOT {
        Err(Status::new_exception(ExceptionCode::SECURITY, None))
    } else {
        Ok(())
    }
}
