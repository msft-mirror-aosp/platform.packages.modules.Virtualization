/*
 * Copyright 2024 The Android Open Source Project
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
package com.android.microdroid.test;

import static com.google.common.truth.Truth.assertWithMessage;

import android.system.virtualmachine.VirtualMachineManager;

import com.android.compatibility.common.util.CddTest;
import com.android.microdroid.test.device.MicrodroidDeviceTestBase;

import org.junit.Ignore;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.junit.runners.JUnit4;

/**
 * Test the advertised AVF capabilities include the ability to start some type of VM.
 *
 * <p>Tests in MicrodroidTests run on either protected or non-protected VMs, provided they are
 * supported. If neither is they are all skipped. So we need a separate test (that doesn't call
 * {@link #prepareTestSetup}) to make sure that at least one of these is available.
 */
@RunWith(JUnit4.class)
public class MicrodroidCapabilitiesTest extends MicrodroidDeviceTestBase {
    @Test
    @CddTest(requirements = {"9.17/C-1-1", "9.17/C-2-1"})
    @Ignore("b/326092480")
    public void supportForProtectedOrNonProtectedVms() {
        assumeSupportedDevice();

        // (There's a test for devices that don't expose the system feature over in
        // NoMicrodroidTest.)
        assumeFeatureVirtualizationFramework();

        int capabilities = getVirtualMachineManager().getCapabilities();
        int vmCapabilities =
                capabilities
                        & (VirtualMachineManager.CAPABILITY_PROTECTED_VM
                                | VirtualMachineManager.CAPABILITY_NON_PROTECTED_VM);
        assertWithMessage(
                        "A device that has FEATURE_VIRTUALIZATION_FRAMEWORK must support at least"
                                + " one of protected or non-protected VMs")
                .that(vmCapabilities)
                .isNotEqualTo(0);
    }
}
