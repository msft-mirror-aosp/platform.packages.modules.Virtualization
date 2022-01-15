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

package com.android.virt.fs;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertTrue;
import static org.junit.Assume.assumeFalse;

import android.platform.test.annotations.RootPermissionTest;
import android.virt.test.CommandRunner;
import android.virt.test.VirtualizationTestCaseBase;

import com.android.compatibility.common.util.PollingCheck;
import com.android.tradefed.device.DeviceNotAvailableException;
import com.android.tradefed.device.ITestDevice;
import com.android.tradefed.invoker.TestInformation;
import com.android.tradefed.log.LogUtil.CLog;
import com.android.tradefed.testtype.DeviceJUnit4ClassRunner;
import com.android.tradefed.testtype.junit4.AfterClassWithInfo;
import com.android.tradefed.testtype.junit4.BeforeClassWithInfo;
import com.android.tradefed.util.CommandResult;
import com.android.tradefed.util.CommandStatus;

import org.junit.After;
import org.junit.AssumptionViolatedException;
import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;

import java.util.Optional;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicBoolean;

@RootPermissionTest
@RunWith(DeviceJUnit4ClassRunner.class)
public final class AuthFsHostTest extends VirtualizationTestCaseBase {

    /** Test directory on Android where data are located */
    private static final String TEST_DIR = "/data/local/tmp/authfs";

    /** Output directory where the test can generate output on Android */
    private static final String TEST_OUTPUT_DIR = "/data/local/tmp/authfs/output_dir";

    /** Path to open_then_run on Android */
    private static final String OPEN_THEN_RUN_BIN = "/data/local/tmp/open_then_run";

    /** Mount point of authfs on Microdroid during the test */
    private static final String MOUNT_DIR = "/data/local/tmp";

    /** Path to fd_server on Android */
    private static final String FD_SERVER_BIN = "/apex/com.android.virt/bin/fd_server";

    /** Path to authfs on Microdroid */
    private static final String AUTHFS_BIN = "/system/bin/authfs";

    /** Idsig paths to be created for each APK in the "extra_apks" of vm_config_extra_apk.json. */
    private static final String[] EXTRA_IDSIG_PATHS = new String[] {
        TEST_DIR + "BuildManifest.apk.idsig",
    };

    /** Build manifest path in the VM. 0 is the index of extra_apks in vm_config_extra_apk.json. */
    private static final String BUILD_MANIFEST_PATH = "/mnt/extra-apk/0/assets/build_manifest.pb";

    /** Plenty of time for authfs to get ready */
    private static final int AUTHFS_INIT_TIMEOUT_MS = 3000;

    /** FUSE's magic from statfs(2) */
    private static final String FUSE_SUPER_MAGIC_HEX = "65735546";

    // fs-verity digest (sha256) of testdata/input.{4k, 4k1, 4m}
    private static final String DIGEST_4K =
            "sha256-9828cd65f4744d6adda216d3a63d8205375be485bfa261b3b8153d3358f5a576";
    private static final String DIGEST_4K1 =
            "sha256-3c70dcd4685ed256ebf1ef116c12e472f35b5017eaca422c0483dadd7d0b5a9f";
    private static final String DIGEST_4M =
            "sha256-f18a268d565348fb4bbf11f10480b198f98f2922eb711de149857b3cecf98a8d";

    private static final int VMADDR_CID_HOST = 2;

    private static CommandRunner sAndroid;
    private static String sCid;
    private static boolean sAssumptionFailed;

    private ExecutorService mThreadPool = Executors.newCachedThreadPool();

    @BeforeClassWithInfo
    public static void beforeClassWithDevice(TestInformation testInfo)
            throws DeviceNotAvailableException {
        assertNotNull(testInfo.getDevice());
        ITestDevice androidDevice = testInfo.getDevice();
        sAndroid = new CommandRunner(androidDevice);

        try {
            testIfDeviceIsCapable(androidDevice);
        } catch (AssumptionViolatedException e) {
            // NB: The assumption exception is NOT handled by the test infra when it is thrown from
            // a class method (see b/37502066). This has not only caused the loss of log, but also
            // prevented the test cases to be reported at all and thus confused the test infra.
            //
            // Since we want to avoid the big overhead to start the VM repeatedly on CF, let's catch
            // AssumptionViolatedException and emulate it artifitially.
            CLog.e("Assumption failed: " + e);
            sAssumptionFailed = true;
            return;
        }

        prepareVirtualizationTestSetup(androidDevice);

        // For each test case, boot and adb connect to a new Microdroid
        CLog.i("Starting the shared VM");
        final String apkName = "MicrodroidTestApp.apk";
        final String packageName = "com.android.microdroid.test";
        final String configPath = "assets/vm_config_extra_apk.json"; // path inside the APK
        sCid =
                startMicrodroid(
                        androidDevice,
                        testInfo.getBuildInfo(),
                        apkName,
                        packageName,
                        EXTRA_IDSIG_PATHS,
                        configPath,
                        /* debug */ true,
                        /* use default memoryMib */ 0,
                        Optional.empty(),
                        Optional.empty());
        adbConnectToMicrodroid(androidDevice, sCid);

        // Root because authfs (started from shell in this test) currently require root to open
        // /dev/fuse and mount the FUSE.
        rootMicrodroid();
    }

    @AfterClassWithInfo
    public static void afterClassWithDevice(TestInformation testInfo)
            throws DeviceNotAvailableException {
        assertNotNull(sAndroid);

        if (sCid != null) {
            CLog.i("Shutting down shared VM");
            shutdownMicrodroid(sAndroid.getDevice(), sCid);
            sCid = null;
        }

        cleanUpVirtualizationTestSetup(sAndroid.getDevice());
        sAndroid = null;
    }

    @Before
    public void setUp() throws Exception {
        assumeFalse(sAssumptionFailed);
        sAndroid.run("mkdir " + TEST_OUTPUT_DIR);
    }

    @After
    public void tearDown() throws Exception {
        sAndroid.tryRun("killall fd_server");
        sAndroid.run("rm -rf " + TEST_OUTPUT_DIR);

        tryRunOnMicrodroid("killall authfs");
        tryRunOnMicrodroid("umount " + MOUNT_DIR);
    }

    @Test
    public void testReadWithFsverityVerification_RemoteFile() throws Exception {
        // Setup
        runFdServerOnAndroid(
                "--open-ro 3:input.4m --open-ro 4:input.4m.fsv_meta --open-ro 6:input.4m",
                "--ro-fds 3:4 --ro-fds 6");

        runAuthFsOnMicrodroid(
                "--remote-ro-file-unverified 6 --remote-ro-file 3:" + DIGEST_4M + " --cid "
                        + VMADDR_CID_HOST);

        // Action
        String actualHashUnverified4m = computeFileHashOnMicrodroid(MOUNT_DIR + "/6");
        String actualHash4m = computeFileHashOnMicrodroid(MOUNT_DIR + "/3");

        // Verify
        String expectedHash4m = computeFileHashOnAndroid(TEST_DIR + "/input.4m");

        assertEquals("Inconsistent hash from /authfs/6: ", expectedHash4m, actualHashUnverified4m);
        assertEquals("Inconsistent hash from /authfs/3: ", expectedHash4m, actualHash4m);
    }

    // Separate the test from the above simply because exec in shell does not allow open too many
    // files.
    @Test
    public void testReadWithFsverityVerification_RemoteSmallerFile() throws Exception {
        // Setup
        runFdServerOnAndroid(
                "--open-ro 3:input.4k --open-ro 4:input.4k.fsv_meta --open-ro"
                    + " 6:input.4k1 --open-ro 7:input.4k1.fsv_meta",
                "--ro-fds 3:4 --ro-fds 6:7");
        runAuthFsOnMicrodroid(
                "--remote-ro-file 3:" + DIGEST_4K + " --remote-ro-file 6:" + DIGEST_4K1 + " --cid "
                + VMADDR_CID_HOST);

        // Action
        String actualHash4k = computeFileHashOnMicrodroid(MOUNT_DIR + "/3");
        String actualHash4k1 = computeFileHashOnMicrodroid(MOUNT_DIR + "/6");

        // Verify
        String expectedHash4k = computeFileHashOnAndroid(TEST_DIR + "/input.4k");
        String expectedHash4k1 = computeFileHashOnAndroid(TEST_DIR + "/input.4k1");

        assertEquals("Inconsistent hash from /authfs/3: ", expectedHash4k, actualHash4k);
        assertEquals("Inconsistent hash from /authfs/6: ", expectedHash4k1, actualHash4k1);
    }

    @Test
    public void testReadWithFsverityVerification_TamperedMerkleTree() throws Exception {
        // Setup
        runFdServerOnAndroid(
                "--open-ro 3:input.4m --open-ro 4:input.4m.fsv_meta.bad_merkle",
                "--ro-fds 3:4");
        runAuthFsOnMicrodroid("--remote-ro-file 3:" + DIGEST_4M + " --cid " + VMADDR_CID_HOST);

        // Verify
        assertFalse(copyFileOnMicrodroid(MOUNT_DIR + "/3", "/dev/null"));
    }

    @Test
    public void testWriteThroughCorrectly() throws Exception {
        // Setup
        runFdServerOnAndroid("--open-rw 3:" + TEST_OUTPUT_DIR + "/out.file", "--rw-fds 3");
        runAuthFsOnMicrodroid("--remote-new-rw-file 3 --cid " + VMADDR_CID_HOST);

        // Action
        String srcPath = "/system/bin/linker64";
        String destPath = MOUNT_DIR + "/3";
        String backendPath = TEST_OUTPUT_DIR + "/out.file";
        assertTrue(copyFileOnMicrodroid(srcPath, destPath));

        // Verify
        String expectedHash = computeFileHashOnMicrodroid(srcPath);
        expectBackingFileConsistency(destPath, backendPath, expectedHash);
    }

    @Test
    public void testWriteFailedIfDetectsTampering() throws Exception {
        // Setup
        runFdServerOnAndroid("--open-rw 3:" + TEST_OUTPUT_DIR + "/out.file", "--rw-fds 3");
        runAuthFsOnMicrodroid("--remote-new-rw-file 3 --cid " + VMADDR_CID_HOST);

        String srcPath = "/system/bin/linker64";
        String destPath = MOUNT_DIR + "/3";
        String backendPath = TEST_OUTPUT_DIR + "/out.file";
        assertTrue(copyFileOnMicrodroid(srcPath, destPath));

        // Action
        // Tampering with the first 2 4K block of the backing file.
        sAndroid.run("dd if=/dev/zero of=" + backendPath + " bs=1 count=8192");

        // Verify
        // Write to a block partially requires a read back to calculate the new hash. It should fail
        // when the content is inconsistent to the known hash. Use direct I/O to avoid simply
        // writing to the filesystem cache.
        assertFalse(
                writeZerosAtFileOffsetOnMicrodroid(
                        destPath, /* offset */ 0, /* number */ 1024, /* writeThrough */ true));

        // A full 4K write does not require to read back, so write can succeed even if the backing
        // block has already been tampered.
        assertTrue(
                writeZerosAtFileOffsetOnMicrodroid(
                        destPath, /* offset */ 4096, /* number */ 4096, /* writeThrough */ false));

        // Otherwise, a partial write with correct backing file should still succeed.
        assertTrue(
                writeZerosAtFileOffsetOnMicrodroid(
                        destPath, /* offset */ 8192, /* number */ 1024, /* writeThrough */ false));
    }

    @Test
    public void testFileResize() throws Exception {
        // Setup
        runFdServerOnAndroid("--open-rw 3:" + TEST_OUTPUT_DIR + "/out.file", "--rw-fds 3");
        runAuthFsOnMicrodroid("--remote-new-rw-file 3 --cid " + VMADDR_CID_HOST);
        String outputPath = MOUNT_DIR + "/3";
        String backendPath = TEST_OUTPUT_DIR + "/out.file";

        // Action & Verify
        createFileWithOnesOnMicrodroid(outputPath, 10000);
        assertEquals(getFileSizeInBytesOnMicrodroid(outputPath), 10000);
        expectBackingFileConsistency(
                outputPath,
                backendPath,
                "684ad25fdc2bbb80cbc910dd1bde6d5499ccf860ca6ee44704b77ec445271353");

        resizeFileOnMicrodroid(outputPath, 15000);
        assertEquals(getFileSizeInBytesOnMicrodroid(outputPath), 15000);
        expectBackingFileConsistency(
                outputPath,
                backendPath,
                "567c89f62586e0d33369157afdfe99a2fa36cdffb01e91dcdc0b7355262d610d");

        resizeFileOnMicrodroid(outputPath, 5000);
        assertEquals(getFileSizeInBytesOnMicrodroid(outputPath), 5000);
        expectBackingFileConsistency(
                outputPath,
                backendPath,
                "e53130831c13dabff71d5d1797e3aaa467b4b7d32b3b8782c4ff03d76976f2aa");
    }

    @Test
    public void testOutputDirectory_WriteNewFiles() throws Exception {
        // Setup
        String androidOutputDir = TEST_OUTPUT_DIR + "/dir";
        String authfsOutputDir = MOUNT_DIR + "/3";
        sAndroid.run("mkdir " + androidOutputDir);
        runFdServerOnAndroid("--open-dir 3:" + androidOutputDir, "--rw-dirs 3");
        runAuthFsOnMicrodroid("--remote-new-rw-dir 3 --cid " + VMADDR_CID_HOST);

        // Action & Verify
        // Can create a new file to write.
        String expectedAndroidPath = androidOutputDir + "/file";
        String authfsPath = authfsOutputDir + "/file";
        createFileWithOnesOnMicrodroid(authfsPath, 10000);
        assertEquals(getFileSizeInBytesOnMicrodroid(authfsPath), 10000);
        expectBackingFileConsistency(
                authfsPath,
                expectedAndroidPath,
                "684ad25fdc2bbb80cbc910dd1bde6d5499ccf860ca6ee44704b77ec445271353");

        // Regular file operations work, e.g. resize.
        resizeFileOnMicrodroid(authfsPath, 15000);
        assertEquals(getFileSizeInBytesOnMicrodroid(authfsPath), 15000);
        expectBackingFileConsistency(
                authfsPath,
                expectedAndroidPath,
                "567c89f62586e0d33369157afdfe99a2fa36cdffb01e91dcdc0b7355262d610d");
    }

    @Test
    public void testOutputDirectory_MkdirAndWriteFile() throws Exception {
        // Setup
        String androidOutputDir = TEST_OUTPUT_DIR + "/dir";
        String authfsOutputDir = MOUNT_DIR + "/3";
        sAndroid.run("mkdir " + androidOutputDir);
        runFdServerOnAndroid("--open-dir 3:" + androidOutputDir, "--rw-dirs 3");
        runAuthFsOnMicrodroid("--remote-new-rw-dir 3 --cid " + VMADDR_CID_HOST);

        // Action
        // Can create nested directories and can create a file in one.
        runOnMicrodroid("mkdir " + authfsOutputDir + "/new_dir");
        runOnMicrodroid("mkdir -p " + authfsOutputDir + "/we/need/to/go/deeper");
        createFileWithOnesOnMicrodroid(authfsOutputDir + "/new_dir/file1", 10000);
        createFileWithOnesOnMicrodroid(authfsOutputDir + "/we/need/file2", 10000);

        // Verify
        // Directories show up in Android.
        sAndroid.run("test -d " + androidOutputDir + "/new_dir");
        sAndroid.run("test -d " + androidOutputDir + "/we/need/to/go/deeper");
        // Files exist in Android. Hashes on Microdroid and Android are consistent.
        assertEquals(getFileSizeInBytesOnMicrodroid(authfsOutputDir + "/new_dir/file1"), 10000);
        expectBackingFileConsistency(
                authfsOutputDir + "/new_dir/file1",
                androidOutputDir + "/new_dir/file1",
                "684ad25fdc2bbb80cbc910dd1bde6d5499ccf860ca6ee44704b77ec445271353");
        // Same to file in a nested directory.
        assertEquals(getFileSizeInBytesOnMicrodroid(authfsOutputDir + "/we/need/file2"), 10000);
        expectBackingFileConsistency(
                authfsOutputDir + "/we/need/file2",
                androidOutputDir + "/we/need/file2",
                "684ad25fdc2bbb80cbc910dd1bde6d5499ccf860ca6ee44704b77ec445271353");
    }

    @Test
    public void testOutputDirectory_CreateAndTruncateExistingFile() throws Exception {
        // Setup
        String androidOutputDir = TEST_OUTPUT_DIR + "/dir";
        String authfsOutputDir = MOUNT_DIR + "/3";
        sAndroid.run("mkdir " + androidOutputDir);
        runFdServerOnAndroid("--open-dir 3:" + androidOutputDir, "--rw-dirs 3");
        runAuthFsOnMicrodroid("--remote-new-rw-dir 3 --cid " + VMADDR_CID_HOST);

        // Action & Verify
        runOnMicrodroid("echo -n foo > " + authfsOutputDir + "/file");
        assertEquals(getFileSizeInBytesOnMicrodroid(authfsOutputDir + "/file"), 3);
        // Can override a file and write normally.
        createFileWithOnesOnMicrodroid(authfsOutputDir + "/file", 10000);
        assertEquals(getFileSizeInBytesOnMicrodroid(authfsOutputDir + "/file"), 10000);
        expectBackingFileConsistency(
                authfsOutputDir + "/file",
                androidOutputDir + "/file",
                "684ad25fdc2bbb80cbc910dd1bde6d5499ccf860ca6ee44704b77ec445271353");
    }

    @Test
    public void testOutputDirectory_CanDeleteFile() throws Exception {
        // Setup
        String androidOutputDir = TEST_OUTPUT_DIR + "/dir";
        String authfsOutputDir = MOUNT_DIR + "/3";
        sAndroid.run("mkdir " + androidOutputDir);
        runFdServerOnAndroid("--open-dir 3:" + androidOutputDir, "--rw-dirs 3");
        runAuthFsOnMicrodroid("--remote-new-rw-dir 3 --cid " + VMADDR_CID_HOST);

        runOnMicrodroid("echo -n foo > " + authfsOutputDir + "/file");
        runOnMicrodroid("test -f " + authfsOutputDir + "/file");
        sAndroid.run("test -f " + androidOutputDir + "/file");

        // Action & Verify
        runOnMicrodroid("rm " + authfsOutputDir + "/file");
        runOnMicrodroid("test ! -f " + authfsOutputDir + "/file");
        sAndroid.run("test ! -f " + androidOutputDir + "/file");
    }

    @Test
    public void testOutputDirectory_CanDeleteDirectoryOnlyIfEmpty() throws Exception {
        // Setup
        String androidOutputDir = TEST_OUTPUT_DIR + "/dir";
        String authfsOutputDir = MOUNT_DIR + "/3";
        sAndroid.run("mkdir " + androidOutputDir);
        runFdServerOnAndroid("--open-dir 3:" + androidOutputDir, "--rw-dirs 3");
        runAuthFsOnMicrodroid("--remote-new-rw-dir 3 --cid " + VMADDR_CID_HOST);

        runOnMicrodroid("mkdir -p " + authfsOutputDir + "/dir/dir2");
        runOnMicrodroid("echo -n foo > " + authfsOutputDir + "/dir/file");
        sAndroid.run("test -d " + androidOutputDir + "/dir/dir2");

        // Action & Verify
        runOnMicrodroid("rmdir " + authfsOutputDir + "/dir/dir2");
        runOnMicrodroid("test ! -d " + authfsOutputDir + "/dir/dir2");
        sAndroid.run("test ! -d " + androidOutputDir + "/dir/dir2");
        // Can only delete a directory if empty
        assertFailedOnMicrodroid("rmdir " + authfsOutputDir + "/dir");
        runOnMicrodroid("test -d " + authfsOutputDir + "/dir");  // still there
        runOnMicrodroid("rm " + authfsOutputDir + "/dir/file");
        runOnMicrodroid("rmdir " + authfsOutputDir + "/dir");
        runOnMicrodroid("test ! -d " + authfsOutputDir + "/dir");
        sAndroid.run("test ! -d " + androidOutputDir + "/dir");
    }

    @Test
    public void testOutputDirectory_CannotRecreateDirectoryIfNameExists() throws Exception {
        // Setup
        String androidOutputDir = TEST_OUTPUT_DIR + "/dir";
        String authfsOutputDir = MOUNT_DIR + "/3";
        sAndroid.run("mkdir " + androidOutputDir);
        runFdServerOnAndroid("--open-dir 3:" + androidOutputDir, "--rw-dirs 3");
        runAuthFsOnMicrodroid("--remote-new-rw-dir 3 --cid " + VMADDR_CID_HOST);

        runOnMicrodroid("touch " + authfsOutputDir + "/some_file");
        runOnMicrodroid("mkdir " + authfsOutputDir + "/some_dir");
        runOnMicrodroid("touch " + authfsOutputDir + "/some_dir/file");
        runOnMicrodroid("mkdir " + authfsOutputDir + "/some_dir/dir");

        // Action & Verify
        // Cannot create directory if an entry with the same name already exists.
        assertFailedOnMicrodroid("mkdir " + authfsOutputDir + "/some_file");
        assertFailedOnMicrodroid("mkdir " + authfsOutputDir + "/some_dir");
        assertFailedOnMicrodroid("mkdir " + authfsOutputDir + "/some_dir/file");
        assertFailedOnMicrodroid("mkdir " + authfsOutputDir + "/some_dir/dir");
    }

    @Test
    public void testOutputDirectory_WriteToFdOfDeletedFile() throws Exception {
        // Setup
        String authfsOutputDir = MOUNT_DIR + "/3";
        String androidOutputDir = TEST_OUTPUT_DIR + "/dir";
        sAndroid.run("mkdir " + androidOutputDir);
        runFdServerOnAndroid("--open-dir 3:" + androidOutputDir, "--rw-dirs 3");
        runAuthFsOnMicrodroid("--remote-new-rw-dir 3 --cid " + VMADDR_CID_HOST);

        // Create a file with some data. Test the existence.
        String outputPath = authfsOutputDir + "/out";
        String androidOutputPath = androidOutputDir + "/out";
        runOnMicrodroid("echo -n 123 > " + outputPath);
        runOnMicrodroid("test -f " + outputPath);
        sAndroid.run("test -f " + androidOutputPath);

        // Action
        String output = runOnMicrodroid(
                // Open the file for append and read
                "exec 4>>" + outputPath + " 5<" + outputPath + "; "
                // Delete the file from the directory
                + "rm " + outputPath + "; "
                // Append more data to the file descriptor
                + "echo -n 456 >&4; "
                // Print the whole file from the file descriptor
                + "cat <&5");

        // Verify
        // Output contains all written data, while the files are deleted.
        assertEquals("123456", output);
        runOnMicrodroid("test ! -f " + outputPath);
        sAndroid.run("test ! -f " + androidOutputDir + "/out");
    }

    @Test
    public void testInputDirectory_CanReadFile() throws Exception {
        // Setup
        String authfsInputDir = MOUNT_DIR + "/3";
        runFdServerOnAndroid("--open-dir 3:/system", "--ro-dirs 3");
        runAuthFsOnMicrodroid("--remote-ro-dir 3:" + BUILD_MANIFEST_PATH + ":system/ --cid "
                + VMADDR_CID_HOST);

        // Action
        String actualHash =
                computeFileHashOnMicrodroid(authfsInputDir + "/system/framework/framework.jar");

        // Verify
        String expectedHash = computeFileHashOnAndroid("/system/framework/framework.jar");
        assertEquals("Expect consistent hash through /authfs/3: ", expectedHash, actualHash);
    }

    @Test
    public void testInputDirectory_OnlyAllowlistedFilesExist() throws Exception {
        // Setup
        String authfsInputDir = MOUNT_DIR + "/3";
        runFdServerOnAndroid("--open-dir 3:/system", "--ro-dirs 3");
        runAuthFsOnMicrodroid("--remote-ro-dir 3:" + BUILD_MANIFEST_PATH + ":system/ --cid "
                + VMADDR_CID_HOST);

        // Verify
        runOnMicrodroid("test -f " + authfsInputDir + "/system/framework/services.jar");
        assertFailedOnMicrodroid("test -f " + authfsInputDir + "/system/bin/sh");
    }

    @Test
    public void testReadOutputDirectory() throws Exception {
        // Setup
        runFdServerOnAndroid("--open-dir 3:" + TEST_OUTPUT_DIR, "--rw-dirs 3");
        runAuthFsOnMicrodroid("--remote-new-rw-dir 3 --cid " + VMADDR_CID_HOST);

        // Action
        String authfsOutputDir = MOUNT_DIR + "/3";
        runOnMicrodroid("mkdir -p " + authfsOutputDir + "/dir/dir2/dir3");
        runOnMicrodroid("touch " + authfsOutputDir + "/dir/dir2/dir3/file1");
        runOnMicrodroid("touch " + authfsOutputDir + "/dir/dir2/dir3/file2");
        runOnMicrodroid("touch " + authfsOutputDir + "/dir/dir2/dir3/file3");
        runOnMicrodroid("touch " + authfsOutputDir + "/file");

        // Verify
        String[] actual = runOnMicrodroid("cd " + authfsOutputDir + "; find |sort").split("\n");
        String[] expected = new String[] {
                ".",
                "./dir",
                "./dir/dir2",
                "./dir/dir2/dir3",
                "./dir/dir2/dir3/file1",
                "./dir/dir2/dir3/file2",
                "./dir/dir2/dir3/file3",
                "./file"};
        assertEquals(expected, actual);

        // Add more entries.
        runOnMicrodroid("mkdir -p " + authfsOutputDir + "/dir2");
        runOnMicrodroid("touch " + authfsOutputDir + "/file2");
        // Check new entries. Also check that the types are correct.
        actual = runOnMicrodroid(
                "cd " + authfsOutputDir + "; find -maxdepth 1 -type f |sort").split("\n");
        expected = new String[] {"./file", "./file2"};
        assertEquals(expected, actual);
        actual = runOnMicrodroid(
                "cd " + authfsOutputDir + "; find -maxdepth 1 -type d |sort").split("\n");
        expected = new String[] {".", "./dir", "./dir2"};
        assertEquals(expected, actual);
    }

    @Test
    public void testChmod_File() throws Exception {
        // Setup
        runFdServerOnAndroid("--open-rw 3:" + TEST_OUTPUT_DIR + "/file", "--rw-fds 3");
        runAuthFsOnMicrodroid("--remote-new-rw-file 3 --cid " + VMADDR_CID_HOST);

        // Action & Verify
        // Change mode
        runOnMicrodroid("chmod 321 " + MOUNT_DIR + "/3");
        expectFileMode("--wx-w---x", MOUNT_DIR + "/3", TEST_OUTPUT_DIR + "/file");
        // Can't set the disallowed bits
        assertFailedOnMicrodroid("chmod +s " + MOUNT_DIR + "/3");
        assertFailedOnMicrodroid("chmod +t " + MOUNT_DIR + "/3");
    }

    @Test
    public void testChmod_Dir() throws Exception {
        // Setup
        runFdServerOnAndroid("--open-dir 3:" + TEST_OUTPUT_DIR, "--rw-dirs 3");
        runAuthFsOnMicrodroid("--remote-new-rw-dir 3 --cid " + VMADDR_CID_HOST);

        // Action & Verify
        String authfsOutputDir = MOUNT_DIR + "/3";
        // Create with umask
        runOnMicrodroid("umask 000; mkdir " + authfsOutputDir + "/dir");
        runOnMicrodroid("umask 022; mkdir " + authfsOutputDir + "/dir/dir2");
        expectFileMode("drwxrwxrwx", authfsOutputDir + "/dir", TEST_OUTPUT_DIR + "/dir");
        expectFileMode("drwxr-xr-x", authfsOutputDir + "/dir/dir2", TEST_OUTPUT_DIR + "/dir/dir2");
        // Change mode
        runOnMicrodroid("chmod -w " + authfsOutputDir + "/dir/dir2");
        expectFileMode("dr-xr-xr-x", authfsOutputDir + "/dir/dir2", TEST_OUTPUT_DIR + "/dir/dir2");
        runOnMicrodroid("chmod 321 " + authfsOutputDir + "/dir");
        expectFileMode("d-wx-w---x", authfsOutputDir + "/dir", TEST_OUTPUT_DIR + "/dir");
        // Can't set the disallowed bits
        assertFailedOnMicrodroid("chmod +s " + authfsOutputDir + "/dir/dir2");
        assertFailedOnMicrodroid("chmod +t " + authfsOutputDir + "/dir");
    }

    @Test
    public void testChmod_FileInOutputDirectory() throws Exception {
        // Setup
        runFdServerOnAndroid("--open-dir 3:" + TEST_OUTPUT_DIR, "--rw-dirs 3");
        runAuthFsOnMicrodroid("--remote-new-rw-dir 3 --cid " + VMADDR_CID_HOST);

        // Action & Verify
        String authfsOutputDir = MOUNT_DIR + "/3";
        // Create with umask
        runOnMicrodroid("umask 000; echo -n foo > " + authfsOutputDir + "/file");
        runOnMicrodroid("umask 022; echo -n foo > " + authfsOutputDir + "/file2");
        expectFileMode("-rw-rw-rw-", authfsOutputDir + "/file", TEST_OUTPUT_DIR + "/file");
        expectFileMode("-rw-r--r--", authfsOutputDir + "/file2", TEST_OUTPUT_DIR + "/file2");
        // Change mode
        runOnMicrodroid("chmod -w " + authfsOutputDir + "/file");
        expectFileMode("-r--r--r--", authfsOutputDir + "/file", TEST_OUTPUT_DIR + "/file");
        runOnMicrodroid("chmod 321 " + authfsOutputDir + "/file2");
        expectFileMode("--wx-w---x", authfsOutputDir + "/file2", TEST_OUTPUT_DIR + "/file2");
        // Can't set the disallowed bits
        assertFailedOnMicrodroid("chmod +s " + authfsOutputDir + "/file");
        assertFailedOnMicrodroid("chmod +t " + authfsOutputDir + "/file2");
    }

    @Test
    public void testStatfs() throws Exception {
        // Setup
        runFdServerOnAndroid("--open-dir 3:" + TEST_OUTPUT_DIR, "--rw-dirs 3");
        runAuthFsOnMicrodroid("--remote-new-rw-dir 3 --cid " + VMADDR_CID_HOST);

        // Verify
        // Magic matches. Has only 2 inodes (root and "/3").
        assertEquals(
                FUSE_SUPER_MAGIC_HEX + " 2", runOnMicrodroid("stat -f -c '%t %c' " + MOUNT_DIR));
    }

    private void expectBackingFileConsistency(
            String authFsPath, String backendPath, String expectedHash)
            throws DeviceNotAvailableException {
        String hashOnAuthFs = computeFileHashOnMicrodroid(authFsPath);
        assertEquals("File hash is different to expectation", expectedHash, hashOnAuthFs);

        String hashOfBackingFile = computeFileHashOnAndroid(backendPath);
        assertEquals(
                "Inconsistent file hash on the backend storage", hashOnAuthFs, hashOfBackingFile);
    }

    private String computeFileHashOnMicrodroid(String path) {
        String result = runOnMicrodroid("sha256sum " + path);
        String[] tokens = result.split("\\s");
        if (tokens.length > 0) {
            return tokens[0];
        } else {
            CLog.e("Unrecognized output by sha256sum: " + result);
            return "";
        }
    }

    private boolean copyFileOnMicrodroid(String src, String dest)
            throws DeviceNotAvailableException {
        // TODO(b/182576497): cp returns error because close(2) returns ENOSYS in the current authfs
        // implementation. We should probably fix that since programs can expect close(2) return 0.
        String cmd = "cat " + src + " > " + dest;
        return tryRunOnMicrodroid(cmd) != null;
    }

    private String computeFileHashOnAndroid(String path) throws DeviceNotAvailableException {
        String result = sAndroid.run("sha256sum " + path);
        String[] tokens = result.split("\\s");
        if (tokens.length > 0) {
            return tokens[0];
        } else {
            CLog.e("Unrecognized output by sha256sum: " + result);
            return "";
        }
    }

    private void expectFileMode(String expected, String microdroidPath, String androidPath)
            throws DeviceNotAvailableException {
        String actual = runOnMicrodroid("stat -c '%A' " + microdroidPath);
        assertEquals("Inconsistent mode for " + microdroidPath, expected, actual);

        actual = sAndroid.run("stat -c '%A' " + androidPath);
        assertEquals("Inconsistent mode for " + androidPath + " (android)", expected, actual);
    }

    private void resizeFileOnMicrodroid(String path, long size) {
        runOnMicrodroid("truncate -c -s " + size + " " + path);
    }

    private long getFileSizeInBytesOnMicrodroid(String path) {
        return Long.parseLong(runOnMicrodroid("stat -c '%s' " + path));
    }

    private void createFileWithOnesOnMicrodroid(String filePath, long numberOfOnes) {
        runOnMicrodroid(
                "yes $'\\x01' | tr -d '\\n' | dd bs=1 count=" + numberOfOnes + " of=" + filePath);
    }

    private boolean writeZerosAtFileOffsetOnMicrodroid(
            String filePath, long offset, long numberOfZeros, boolean writeThrough) {
        String cmd = "dd if=/dev/zero of=" + filePath + " bs=1 count=" + numberOfZeros;
        if (offset > 0) {
            cmd += " skip=" + offset;
        }
        if (writeThrough) {
            cmd += " direct";
        }
        CommandResult result = runOnMicrodroidForResult(cmd);
        return result.getStatus() == CommandStatus.SUCCESS;
    }

    private void runAuthFsOnMicrodroid(String flags) {
        String cmd = AUTHFS_BIN + " " + MOUNT_DIR + " " + flags;

        AtomicBoolean starting = new AtomicBoolean(true);
        mThreadPool.submit(
                () -> {
                    // authfs may fail to start if fd_server is not yet listening on the vsock
                    // ("Error: Invalid raw AIBinder"). Just restart if that happens.
                    while (starting.get()) {
                        CLog.i("Starting authfs");
                        CommandResult result = runOnMicrodroidForResult(cmd);
                        CLog.w("authfs has stopped: " + result);
                    }
                });
        try {
            PollingCheck.waitFor(
                    AUTHFS_INIT_TIMEOUT_MS, () -> isMicrodroidDirectoryOnFuse(MOUNT_DIR));
        } catch (Exception e) {
            // Convert the broad Exception into an unchecked exception to avoid polluting all other
            // methods. waitFor throws Exception because the callback, Callable#call(), has a
            // signature to throw an Exception.
            throw new RuntimeException(e);
        } finally {
            starting.set(false);
        }
    }

    private void runFdServerOnAndroid(String helperFlags, String fdServerFlags)
            throws DeviceNotAvailableException {
        String cmd =
                "cd "
                        + TEST_DIR
                        + " && "
                        + OPEN_THEN_RUN_BIN
                        + " "
                        + helperFlags
                        + " -- "
                        + FD_SERVER_BIN
                        + " "
                        + fdServerFlags;

        mThreadPool.submit(
                () -> {
                    try {
                        CLog.i("Starting fd_server");
                        CommandResult result = sAndroid.runForResult(cmd);
                        CLog.w("fd_server has stopped: " + result);
                    } catch (DeviceNotAvailableException e) {
                        CLog.e("Error running fd_server", e);
                        throw new RuntimeException(e);
                    }
                });
    }

    private boolean isMicrodroidDirectoryOnFuse(String path) {
        String fs_type = tryRunOnMicrodroid("stat -f -c '%t' " + path);
        return FUSE_SUPER_MAGIC_HEX.equals(fs_type);
    }
}
