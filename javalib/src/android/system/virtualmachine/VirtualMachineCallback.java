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

package android.system.virtualmachine;

import android.annotation.IntDef;
import android.annotation.NonNull;
import android.annotation.Nullable;
import android.os.ParcelFileDescriptor;

import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;

/**
 * Callback interface to get notified with the events from the virtual machine. The methods are
 * executed on a binder thread. Implementations can make blocking calls in the methods.
 *
 * @hide
 */
public interface VirtualMachineCallback {
    /** @hide */
    @Retention(RetentionPolicy.SOURCE)
    @IntDef({
        ERROR_UNKNOWN,
        ERROR_PAYLOAD_VERIFICATION_FAILED,
        ERROR_PAYLOAD_CHANGED,
        ERROR_PAYLOAD_INVALID_CONFIG
    })
    @interface ErrorCode {}

    /** Error code for all other errors not listed below. */
    int ERROR_UNKNOWN = 0;

    /**
     * Error code indicating that the payload can't be verified due to various reasons (e.g invalid
     * merkle tree, invalid formats, etc).
     */
    int ERROR_PAYLOAD_VERIFICATION_FAILED = 1;

    /** Error code indicating that the payload is verified, but has changed since the last boot. */
    int ERROR_PAYLOAD_CHANGED = 2;

    /** Error code indicating that the payload config is invalid. */
    int ERROR_PAYLOAD_INVALID_CONFIG = 3;

    /** @hide */
    @Retention(RetentionPolicy.SOURCE)
    @IntDef({
        DEATH_REASON_VIRTUALIZATIONSERVICE_DIED,
        DEATH_REASON_INFRASTRUCTURE_ERROR,
        DEATH_REASON_KILLED,
        DEATH_REASON_UNKNOWN,
        DEATH_REASON_SHUTDOWN,
        DEATH_REASON_ERROR,
        DEATH_REASON_REBOOT,
        DEATH_REASON_CRASH,
        DEATH_REASON_HANGUP,
    })
    @interface DeathReason {}

    /**
     * virtualizationservice itself died, taking the VM down with it. This is a negative number to
     * avoid conflicting with the other death reasons which match the ones in the AIDL interface.
     */
    int DEATH_REASON_VIRTUALIZATIONSERVICE_DIED = -1;

    /** There was an error waiting for the VM. */
    int DEATH_REASON_INFRASTRUCTURE_ERROR = 0;

    /** The VM was killed. */
    int DEATH_REASON_KILLED = 1;

    /** The VM died for an unknown reason. */
    int DEATH_REASON_UNKNOWN = 2;

    /** The VM requested to shut down. */
    int DEATH_REASON_SHUTDOWN = 3;

    /** crosvm had an error starting the VM. */
    int DEATH_REASON_ERROR = 4;

    /** The VM requested to reboot, possibly as the result of a kernel panic. */
    int DEATH_REASON_REBOOT = 5;

    /** The VM or crosvm crashed. */
    int DEATH_REASON_CRASH = 6;

    /** The pVM firmware failed to verify the VM because the public key doesn't match. */
    int DEATH_REASON_PVM_FIRMWARE_PUBLIC_KEY_MISMATCH = 7;

    /** The pVM firmware failed to verify the VM because the instance image changed. */
    int DEATH_REASON_PVM_FIRMWARE_INSTANCE_IMAGE_CHANGED = 8;

    /** The bootloader failed to verify the VM because the public key doesn't match. */
    int DEATH_REASON_BOOTLOADER_PUBLIC_KEY_MISMATCH = 9;

    /** The bootloader failed to verify the VM because the instance image changed. */
    int DEATH_REASON_BOOTLOADER_INSTANCE_IMAGE_CHANGED = 10;

    /** The microdroid failed to connect to VirtualizationService's RPC server. */
    int DEATH_REASON_MICRODROID_FAILED_TO_CONNECT_TO_VIRTUALIZATION_SERVICE = 11;

    /** The payload for microdroid is changed. */
    int DEATH_REASON_MICRODROID_PAYLOAD_HAS_CHANGED = 12;

    /** The microdroid failed to verify given payload APK. */
    int DEATH_REASON_MICRODROID_PAYLOAD_VERIFICATION_FAILED = 13;

    /** The VM config for microdroid is invalid (e.g. missing tasks). */
    int DEATH_REASON_MICRODROID_INVALID_PAYLOAD_CONFIG = 14;

    /** There was a runtime error while running microdroid manager. */
    int DEATH_REASON_MICRODROID_UNKNOWN_RUNTIME_ERROR = 15;

    /** The VM killed due to hangup */
    int DEATH_REASON_HANGUP = 16;

    /** Called when the payload starts in the VM. */
    void onPayloadStarted(@NonNull VirtualMachine vm, @Nullable ParcelFileDescriptor stream);

    /** Called when the payload in the VM is ready to serve. */
    void onPayloadReady(@NonNull VirtualMachine vm);

    /** Called when the payload has finished in the VM. */
    void onPayloadFinished(@NonNull VirtualMachine vm, int exitCode);

    /** Called when an error occurs in the VM. */
    void onError(@NonNull VirtualMachine vm, @ErrorCode int errorCode, @NonNull String message);

    /** Called when the VM died. */
    void onDied(@NonNull VirtualMachine vm, @DeathReason int reason);

    /** Called when kernel panic occurs and as a result ramdump is generated from the VM. */
    void onRamdump(@NonNull VirtualMachine vm, @NonNull ParcelFileDescriptor ramdump);
}
