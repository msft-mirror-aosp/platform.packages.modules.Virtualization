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

package android.virt.test;

import static org.hamcrest.CoreMatchers.is;
import static org.junit.Assert.assertThat;

import com.android.tradefed.testtype.DeviceJUnit4ClassRunner;

import org.junit.After;
import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;

@RunWith(DeviceJUnit4ClassRunner.class)
public class MicrodroidTestCase extends VirtualizationTestCaseBase {
    private static final String APK_NAME = "MicrodroidTestApp.apk";
    private static final String PACKAGE_NAME = "com.android.microdroid.test";

    @Test
    public void testMicrodroidBoots() throws Exception {
        final String configPath = "assets/vm_config.json"; // path inside the APK
        final String cid =
                startMicrodroid(
                        getDevice(),
                        getBuild(),
                        APK_NAME,
                        PACKAGE_NAME,
                        configPath,
                        /* debug */ false);
        adbConnectToMicrodroid(getDevice(), cid);

        // Test writing to /data partition
        runOnMicrodroid("echo MicrodroidTest > /data/local/tmp/test.txt");
        assertThat(runOnMicrodroid("cat /data/local/tmp/test.txt"), is("MicrodroidTest"));

        // Check if the APK & its idsig partitions exist
        final String apkPartition = "/dev/block/by-name/microdroid-apk";
        assertThat(runOnMicrodroid("ls", apkPartition), is(apkPartition));
        final String apkIdsigPartition = "/dev/block/by-name/microdroid-apk-idsig";
        assertThat(runOnMicrodroid("ls", apkIdsigPartition), is(apkIdsigPartition));
        // Check the vm-instance partition as well
        final String vmInstancePartition = "/dev/block/by-name/vm-instance";
        assertThat(runOnMicrodroid("ls", vmInstancePartition), is(vmInstancePartition));

        // Check if the native library in the APK is has correct filesystem info
        final String[] abis = runOnMicrodroid("getprop", "ro.product.cpu.abilist").split(",");
        assertThat(abis.length, is(1));
        final String testLib = "/mnt/apk/lib/" + abis[0] + "/MicrodroidTestNativeLib.so";
        final String label = "u:object_r:system_file:s0";
        assertThat(runOnMicrodroid("ls", "-Z", testLib), is(label + " " + testLib));

        // Check if the command in vm_config.json was executed by examining the side effect of the
        // command
        assertThat(runOnMicrodroid("getprop", "debug.microdroid.app.run"), is("true"));
        assertThat(runOnMicrodroid("getprop", "debug.microdroid.app.sublib.run"), is("true"));

        // Check that keystore was found by the payload. Wait until the property is set.
        tryRunOnMicrodroid("watch -e \"getprop debug.microdroid.test.keystore | grep '^$'\"");
        assertThat(runOnMicrodroid("getprop", "debug.microdroid.test.keystore"), is("PASS"));

        // Check that no denials have happened so far
        assertThat(runOnMicrodroid("logcat -d -e 'avc:[[:space:]]{1,2}denied'"), is(""));

        shutdownMicrodroid(getDevice(), cid);
    }

    @Test
    public void testDebugMode() throws Exception {
        final String configPath = "assets/vm_config.json"; // path inside the APK
        final boolean debug = true;
        final String cid =
                startMicrodroid(getDevice(), getBuild(), APK_NAME, PACKAGE_NAME, configPath, debug);
        adbConnectToMicrodroid(getDevice(), cid);

        assertThat(runOnMicrodroid("getenforce"), is("Permissive"));

        shutdownMicrodroid(getDevice(), cid);
    }

    @Before
    public void setUp() throws Exception {
        testIfDeviceIsCapable(getDevice());

        prepareVirtualizationTestSetup(getDevice());

        getDevice().installPackage(findTestFile(APK_NAME), /* reinstall */ false);

        // clear the log
        getDevice().executeShellV2Command("logcat -c");
    }

    @After
    public void shutdown() throws Exception {
        cleanUpVirtualizationTestSetup(getDevice());

        getDevice().uninstallPackage(PACKAGE_NAME);
    }
}
