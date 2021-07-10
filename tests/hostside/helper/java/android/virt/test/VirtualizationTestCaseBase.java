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
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

import com.android.compatibility.common.tradefed.build.CompatibilityBuildHelper;
import com.android.tradefed.build.IBuildInfo;
import com.android.tradefed.device.DeviceNotAvailableException;
import com.android.tradefed.device.ITestDevice;
import com.android.tradefed.log.LogUtil.CLog;
import com.android.tradefed.testtype.junit4.BaseHostJUnit4Test;
import com.android.tradefed.util.CommandResult;
import com.android.tradefed.util.CommandStatus;
import com.android.tradefed.util.RunUtil;

import java.io.File;
import java.io.FileNotFoundException;
import java.util.Arrays;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

public abstract class VirtualizationTestCaseBase extends BaseHostJUnit4Test {
    private static final String TEST_ROOT = "/data/local/tmp/virt/";
    private static final String VIRT_APEX = "/apex/com.android.virt/";
    private static final int TEST_VM_ADB_PORT = 8000;
    private static final String MICRODROID_SERIAL = "localhost:" + TEST_VM_ADB_PORT;

    // This is really slow on GCE (2m 40s) but fast on localhost or actual Android phones (< 10s)
    // Set the maximum timeout value big enough.
    private static final long MICRODROID_BOOT_TIMEOUT_MINUTES = 5;

    private static final long MICRODROID_ADB_CONNECT_TIMEOUT_MINUTES = 5;

    public static void prepareVirtualizationTestSetup(ITestDevice androidDevice)
            throws DeviceNotAvailableException {
        CommandRunner android = new CommandRunner(androidDevice);

        // kill stale crosvm processes
        android.tryRun("killall", "crosvm");

        // disconnect from microdroid
        tryRunOnHost("adb", "disconnect", MICRODROID_SERIAL);
    }

    public static void cleanUpVirtualizationTestSetup(ITestDevice androidDevice)
            throws DeviceNotAvailableException {
        CommandRunner android = new CommandRunner(androidDevice);

        // disconnect from microdroid
        tryRunOnHost("adb", "disconnect", MICRODROID_SERIAL);

        // kill stale VMs and directories
        android.tryRun("killall", "crosvm");
        android.tryRun("rm", "-rf", "/data/misc/virtualizationservice/*");
        android.tryRun("stop", "virtualizationservice");
    }

    public static void testIfDeviceIsCapable(ITestDevice androidDevice)
            throws DeviceNotAvailableException {
        CommandRunner android = new CommandRunner(androidDevice);

        // Checks the preconditions to run microdroid. If the condition is not satisfied
        // don't run the test (instead of failing)
        android.assumeSuccess("ls /dev/kvm");
        android.assumeSuccess("ls /dev/vhost-vsock");
        android.assumeSuccess("ls /apex/com.android.virt/bin/crosvm");
    }

    // Run an arbitrary command in the host side and returns the result
    private static String runOnHost(String... cmd) {
        return runOnHostWithTimeout(10000, cmd);
    }

    // Same as runOnHost, but failure is not an error
    private static String tryRunOnHost(String... cmd) {
        final long timeout = 10000;
        CommandResult result = RunUtil.getDefault().runTimedCmd(timeout, cmd);
        return result.getStdout().trim();
    }

    // Same as runOnHost, but with custom timeout
    private static String runOnHostWithTimeout(long timeoutMillis, String... cmd) {
        assertTrue(timeoutMillis >= 0);
        CommandResult result = RunUtil.getDefault().runTimedCmd(timeoutMillis, cmd);
        assertThat(result.getStatus(), is(CommandStatus.SUCCESS));
        return result.getStdout().trim();
    }

    // Run a shell command on Microdroid
    public static String runOnMicrodroid(String... cmd) {
        CommandResult result = runOnMicrodroidForResult(cmd);
        if (result.getStatus() != CommandStatus.SUCCESS) {
            fail(join(cmd) + " has failed: " + result);
        }
        return result.getStdout().trim();
    }

    // Same as runOnMicrodroid, but returns null on error.
    public static String tryRunOnMicrodroid(String... cmd) {
        CommandResult result = runOnMicrodroidForResult(cmd);
        if (result.getStatus() == CommandStatus.SUCCESS) {
            return result.getStdout().trim();
        } else {
            CLog.d(join(cmd) + " has failed (but ok): " + result);
            return null;
        }
    }

    public static CommandResult runOnMicrodroidForResult(String... cmd) {
        final long timeout = 30000; // 30 sec. Microdroid is extremely slow on GCE-on-CF.
        return RunUtil.getDefault()
                .runTimedCmd(timeout, "adb", "-s", MICRODROID_SERIAL, "shell", join(cmd));
    }

    private static String join(String... strs) {
        return String.join(" ", Arrays.asList(strs));
    }

    public File findTestFile(String name) {
        return findTestFile(getBuild(), name);
    }

    private static File findTestFile(IBuildInfo buildInfo, String name) {
        try {
            return (new CompatibilityBuildHelper(buildInfo)).getTestFile(name);
        } catch (FileNotFoundException e) {
            fail("Missing test file: " + name);
            return null;
        }
    }

    public static String startMicrodroid(
            ITestDevice androidDevice,
            IBuildInfo buildInfo,
            String apkName,
            String packageName,
            String configPath,
            boolean debug)
            throws DeviceNotAvailableException {
        CommandRunner android = new CommandRunner(androidDevice);

        // Install APK
        File apkFile = findTestFile(buildInfo, apkName);
        androidDevice.installPackage(apkFile, /* reinstall */ true);

        // Get the path to the installed apk. Note that
        // getDevice().getAppPackageInfo(...).getCodePath() doesn't work due to the incorrect
        // parsing of the "=" character. (b/190975227). So we use the `pm path` command directly.
        String apkPath = android.run("pm", "path", packageName);
        assertTrue(apkPath.startsWith("package:"));
        apkPath = apkPath.substring("package:".length());

        // Push the idsig file to the device
        File idsigOnHost = findTestFile(buildInfo, apkName + ".idsig");
        final String apkIdsigPath = TEST_ROOT + apkName + ".idsig";
        androidDevice.pushFile(idsigOnHost, apkIdsigPath);

        final String logPath = TEST_ROOT + "log.txt";
        final String debugFlag = debug ? "--debug " : "";

        // Run the VM
        android.run("start", "virtualizationservice");
        String ret =
                android.run(
                        VIRT_APEX + "bin/vm",
                        "run-app",
                        "--daemonize",
                        "--log " + logPath,
                        debugFlag,
                        apkPath,
                        apkIdsigPath,
                        configPath);

        // Redirect log.txt to logd using logwrapper
        ExecutorService executor = Executors.newFixedThreadPool(1);
        executor.execute(
                () -> {
                    try {
                        // Keep redirecting sufficiently long enough
                        android.runWithTimeout(
                                MICRODROID_BOOT_TIMEOUT_MINUTES * 60 * 1000,
                                "logwrapper",
                                "tail",
                                "-f",
                                "-n +0",
                                logPath);
                    } catch (Exception e) {
                        // Consume
                    }
                });

        // Retrieve the CID from the vm tool output
        Pattern pattern = Pattern.compile("with CID (\\d+)");
        Matcher matcher = pattern.matcher(ret);
        assertTrue(matcher.find());
        return matcher.group(1);
    }

    public static void shutdownMicrodroid(ITestDevice androidDevice, String cid)
            throws DeviceNotAvailableException {
        CommandRunner android = new CommandRunner(androidDevice);

        // Close the connection before shutting the VM down. Otherwise, b/192660485.
        tryRunOnHost("adb", "disconnect", MICRODROID_SERIAL);
        final String serial = androidDevice.getSerialNumber();
        tryRunOnHost("adb", "-s", serial, "forward", "--remove", "tcp:" + TEST_VM_ADB_PORT);

        // Shutdown the VM
        android.run(VIRT_APEX + "bin/vm", "stop", cid);
    }

    public static void rootMicrodroid() throws DeviceNotAvailableException {
        runOnHost("adb", "-s", MICRODROID_SERIAL, "root");

        // TODO(192660959): Figure out the root cause and remove the sleep. For unknown reason,
        // even though `adb root` actually wait-for-disconnect then wait-for-device, the next
        // `adb -s $MICRODROID_SERIAL shell ...` often fails with "adb: device offline".
        try {
            Thread.sleep(1000);
            runOnHostWithTimeout(
                    MICRODROID_ADB_CONNECT_TIMEOUT_MINUTES * 60 * 1000,
                    "adb",
                    "-s",
                    MICRODROID_SERIAL,
                    "wait-for-device");
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
    }

    // Establish an adb connection to microdroid by letting Android forward the connection to
    // microdroid. Wait until the connection is established and microdroid is booted.
    public static void adbConnectToMicrodroid(ITestDevice androidDevice, String cid)
            throws DeviceNotAvailableException {
        long start = System.currentTimeMillis();
        long timeoutMillis = MICRODROID_ADB_CONNECT_TIMEOUT_MINUTES * 60 * 1000;
        long elapsed = 0;

        final String serial = androidDevice.getSerialNumber();
        final String from = "tcp:" + TEST_VM_ADB_PORT;
        final String to = "vsock:" + cid + ":5555";
        runOnHost("adb", "-s", serial, "forward", from, to);

        boolean disconnected = true;
        while (disconnected) {
            elapsed = System.currentTimeMillis() - start;
            timeoutMillis -= elapsed;
            start = System.currentTimeMillis();
            String ret = runOnHostWithTimeout(timeoutMillis, "adb", "connect", MICRODROID_SERIAL);
            disconnected = ret.equals("failed to connect to " + MICRODROID_SERIAL);
            if (disconnected) {
                // adb demands us to disconnect if the prior connection was a failure.
                runOnHost("adb", "disconnect", MICRODROID_SERIAL);
            }
        }

        elapsed = System.currentTimeMillis() - start;
        timeoutMillis -= elapsed;
        runOnHostWithTimeout(timeoutMillis, "adb", "-s", MICRODROID_SERIAL, "wait-for-device");

        boolean dataAvailable = false;
        while (!dataAvailable && timeoutMillis >= 0) {
            elapsed = System.currentTimeMillis() - start;
            timeoutMillis -= elapsed;
            start = System.currentTimeMillis();
            final String checkCmd = "if [ -d /data/local/tmp ]; then echo 1; fi";
            dataAvailable = runOnMicrodroid(checkCmd).equals("1");
        }

        // Check if it actually booted by reading a sysprop.
        assertThat(runOnMicrodroid("getprop", "ro.hardware"), is("microdroid"));
    }
}
