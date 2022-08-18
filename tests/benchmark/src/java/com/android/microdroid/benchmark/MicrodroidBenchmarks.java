/*
 * Copyright (C) 2022 The Android Open Source Project
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

package com.android.microdroid.benchmark;

import static androidx.test.platform.app.InstrumentationRegistry.getInstrumentation;

import static com.google.common.truth.Truth.assertThat;
import static com.google.common.truth.TruthJUnit.assume;

import android.app.Instrumentation;
import android.os.Bundle;
import android.system.virtualmachine.VirtualMachine;
import android.system.virtualmachine.VirtualMachineConfig;
import android.system.virtualmachine.VirtualMachineConfig.DebugLevel;
import android.system.virtualmachine.VirtualMachineException;

import com.android.microdroid.test.MicrodroidDeviceTestBase;
import com.android.microdroid.testservice.IBenchmarkService;

import org.junit.Before;
import org.junit.Rule;
import org.junit.Test;
import org.junit.rules.Timeout;
import org.junit.runner.RunWith;
import org.junit.runners.Parameterized;

import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.List;

@RunWith(Parameterized.class)
public class MicrodroidBenchmarks extends MicrodroidDeviceTestBase {
    private static final String TAG = "MicrodroidBenchmarks";
    private static final String METRIC_NAME_PREFIX = "avf_perf/microdroid/";
    private static final int VIRTIO_BLK_TRIAL_COUNT = 5;

    @Rule public Timeout globalTimeout = Timeout.seconds(300);

    private static final String APEX_ETC_FS = "/apex/com.android.virt/etc/fs/";
    private static final double SIZE_MB = 1024.0 * 1024.0;
    private static final String MICRODROID_IMG_PREFIX = "microdroid_";
    private static final String MICRODROID_IMG_SUFFIX = ".img";

    @Parameterized.Parameters(name = "protectedVm={0}")
    public static Object[] protectedVmConfigs() {
        return new Object[] {false, true};
    }

    @Parameterized.Parameter public boolean mProtectedVm;

    private Instrumentation mInstrumentation;

    @Before
    public void setup() {
        prepareTestSetup(mProtectedVm);
        mInstrumentation = getInstrumentation();
    }

    private boolean canBootMicrodroidWithMemory(int mem)
            throws VirtualMachineException, InterruptedException, IOException {
        final int trialCount = 5;

        // returns true if succeeded at least once.
        for (int i = 0; i < trialCount; i++) {
            VirtualMachineConfig.Builder builder =
                    mInner.newVmConfigBuilder("assets/vm_config.json");
            VirtualMachineConfig normalConfig =
                    builder.debugLevel(DebugLevel.NONE).memoryMib(mem).build();
            mInner.forceCreateNewVirtualMachine("test_vm_minimum_memory", normalConfig);

            if (tryBootVm(TAG, "test_vm_minimum_memory").payloadStarted) return true;
        }

        return false;
    }

    @Test
    public void testMinimumRequiredRAM()
            throws VirtualMachineException, InterruptedException, IOException {
        assume().withMessage("Skip on CF; too slow").that(isCuttlefish()).isFalse();

        int lo = 16, hi = 512, minimum = 0;
        boolean found = false;

        while (lo <= hi) {
            int mid = (lo + hi) / 2;
            if (canBootMicrodroidWithMemory(mid)) {
                found = true;
                minimum = mid;
                hi = mid - 1;
            } else {
                lo = mid + 1;
            }
        }

        assertThat(found).isTrue();

        Bundle bundle = new Bundle();
        bundle.putInt("avf_perf/microdroid/minimum_required_memory", minimum);
        mInstrumentation.sendStatus(0, bundle);
    }

    @Test
    public void testMicrodroidBootTime()
            throws VirtualMachineException, InterruptedException, IOException {
        assume().withMessage("Skip on CF; too slow").that(isCuttlefish()).isFalse();

        final int trialCount = 10;

        List<Double> vmStartingTimeMetrics = new ArrayList<>();
        List<Double> bootTimeMetrics = new ArrayList<>();
        List<Double> bootloaderTimeMetrics = new ArrayList<>();
        List<Double> kernelBootTimeMetrics = new ArrayList<>();
        List<Double> userspaceBootTimeMetrics = new ArrayList<>();

        for (int i = 0; i < trialCount; i++) {
            VirtualMachineConfig.Builder builder =
                    mInner.newVmConfigBuilder("assets/vm_config.json");

            // To grab boot events from log, set debug mode to FULL
            VirtualMachineConfig normalConfig =
                    builder.debugLevel(DebugLevel.FULL).memoryMib(256).build();
            mInner.forceCreateNewVirtualMachine("test_vm_boot_time", normalConfig);

            BootResult result = tryBootVm(TAG, "test_vm_boot_time");
            assertThat(result.payloadStarted).isTrue();

            final double nanoToMilli = 1000000.0;
            vmStartingTimeMetrics.add(result.getVMStartingElapsedNanoTime() / nanoToMilli);
            bootTimeMetrics.add(result.endToEndNanoTime / nanoToMilli);
            bootloaderTimeMetrics.add(result.getBootloaderElapsedNanoTime() / nanoToMilli);
            kernelBootTimeMetrics.add(result.getKernelElapsedNanoTime() / nanoToMilli);
            userspaceBootTimeMetrics.add(result.getUserspaceElapsedNanoTime() / nanoToMilli);
        }

        reportMetrics(vmStartingTimeMetrics, "vm_starting_time_", "_ms");
        reportMetrics(bootTimeMetrics, "boot_time_", "_ms");
        reportMetrics(bootloaderTimeMetrics, "bootloader_time_", "_ms");
        reportMetrics(kernelBootTimeMetrics, "kernel_boot_time_", "_ms");
        reportMetrics(userspaceBootTimeMetrics, "userspace_boot_time_", "_ms");
    }

    @Test
    public void testMicrodroidImageSize() throws IOException {
        Bundle bundle = new Bundle();
        for (File file : new File(APEX_ETC_FS).listFiles()) {
            String name = file.getName();

            if (!name.startsWith(MICRODROID_IMG_PREFIX) || !name.endsWith(MICRODROID_IMG_SUFFIX)) {
                continue;
            }

            String base =
                    name.substring(
                            MICRODROID_IMG_PREFIX.length(),
                            name.length() - MICRODROID_IMG_SUFFIX.length());
            String metric = METRIC_NAME_PREFIX + "img_size_" + base + "_MB";
            double size = Files.size(file.toPath()) / SIZE_MB;
            bundle.putDouble(metric, size);
        }
        mInstrumentation.sendStatus(0, bundle);
    }

    @Test
    public void testVirtioBlkSeqReadRate() throws Exception {
        testVirtioBlkReadRate(/*isRand=*/ false);
    }

    @Test
    public void testVirtioBlkRandReadRate() throws Exception {
        testVirtioBlkReadRate(/*isRand=*/ true);
    }

    private void testVirtioBlkReadRate(boolean isRand) throws Exception {
        VirtualMachineConfig.Builder builder =
                mInner.newVmConfigBuilder("assets/vm_config_io.json");
        VirtualMachineConfig config = builder.debugLevel(DebugLevel.FULL).build();
        List<Double> readRates = new ArrayList<>();

        for (int i = 0; i < VIRTIO_BLK_TRIAL_COUNT + 1; ++i) {
            if (i == 1) {
                // Clear the first result because when the file was loaded the first time,
                // the data also needs to be loaded from hard drive to host. This is
                // not part of the virtio-blk IO throughput.
                readRates.clear();
            }
            String vmName = "test_vm_io_" + i;
            mInner.forceCreateNewVirtualMachine(vmName, config);
            VirtualMachine vm = mInner.getVirtualMachineManager().get(vmName);
            VirtioBlkVmEventListener listener = new VirtioBlkVmEventListener(readRates, isRand);
            listener.runToFinish(TAG, vm);
        }
        reportMetrics(
                readRates,
                isRand ? "virtio-blk/rand_read_" : "virtio-blk/seq_read_",
                "_mb_per_sec");
    }

    private void reportMetrics(List<Double> data, String base, String suffix) {
        double sum = 0;
        double min = Double.MAX_VALUE;
        double max = Double.MIN_VALUE;
        for (double d : data) {
            sum += d;
            if (min > d) min = d;
            if (max < d) max = d;
        }
        double avg = sum / data.size();
        double sqSum = 0;
        for (double d : data) {
            sqSum += (d - avg) * (d - avg);
        }
        double stdDev = Math.sqrt(sqSum / (data.size() - 1));

        Bundle bundle = new Bundle();
        String prefix = METRIC_NAME_PREFIX + base;
        bundle.putDouble(prefix + "min" + suffix, min);
        bundle.putDouble(prefix + "max" + suffix, max);
        bundle.putDouble(prefix + "average" + suffix, avg);
        bundle.putDouble(prefix + "stdev" + suffix, stdDev);
        mInstrumentation.sendStatus(0, bundle);
    }

    private static class VirtioBlkVmEventListener extends VmEventListener {
        private static final String FILENAME = APEX_ETC_FS + "microdroid_super.img";

        private final long mFileSizeBytes;
        private final List<Double> mReadRates;
        private final boolean mIsRand;

        VirtioBlkVmEventListener(List<Double> readRates, boolean isRand) {
            File file = new File(FILENAME);
            try {
                mFileSizeBytes = Files.size(file.toPath());
            } catch (IOException e) {
                throw new RuntimeException(e);
            }
            assertThat(mFileSizeBytes).isGreaterThan((long) SIZE_MB);
            mReadRates = readRates;
            mIsRand = isRand;
        }

        @Override
        public void onPayloadReady(VirtualMachine vm) {
            try {
                IBenchmarkService benchmarkService =
                        IBenchmarkService.Stub.asInterface(
                                vm.connectToVsockServer(IBenchmarkService.SERVICE_PORT).get());
                double elapsedSeconds =
                        benchmarkService.readFile(FILENAME, mFileSizeBytes, mIsRand);
                double fileSizeMb = mFileSizeBytes / SIZE_MB;
                mReadRates.add(fileSizeMb / elapsedSeconds);
            } catch (Exception e) {
                throw new RuntimeException(e);
            }
            forceStop(vm);
        }
    }
}
