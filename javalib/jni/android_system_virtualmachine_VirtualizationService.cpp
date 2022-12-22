/*
 * Copyright 2022 The Android Open Source Project
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

#define LOG_TAG "VirtualizationService"

#include <android-base/unique_fd.h>
#include <android/binder_ibinder_jni.h>
#include <jni.h>
#include <log/log.h>

#include <string>

#include "common.h"

using namespace android::base;

static constexpr const char VIRTMGR_PATH[] = "/apex/com.android.virt/bin/virtmgr";
static constexpr size_t VIRTMGR_THREADS = 16;

JNIEXPORT jint JNICALL android_system_virtualmachine_VirtualizationService_spawn(
        JNIEnv* env, [[maybe_unused]] jclass clazz) {
    unique_fd serverFd, clientFd;
    if (!Socketpair(SOCK_STREAM, &serverFd, &clientFd)) {
        env->ThrowNew(env->FindClass("android/system/virtualmachine/VirtualMachineException"),
                      ("Failed to create socketpair: " + std::string(strerror(errno))).c_str());
        return -1;
    }

    unique_fd waitFd, readyFd;
    if (!Pipe(&waitFd, &readyFd, 0)) {
        env->ThrowNew(env->FindClass("android/system/virtualmachine/VirtualMachineException"),
                      ("Failed to create pipe: " + std::string(strerror(errno))).c_str());
        return -1;
    }

    if (fork() == 0) {
        // Close client's FDs.
        clientFd.reset();
        waitFd.reset();

        auto strServerFd = std::to_string(serverFd.get());
        auto strReadyFd = std::to_string(readyFd.get());

        execl(VIRTMGR_PATH, VIRTMGR_PATH, "--rpc-server-fd", strServerFd.c_str(), "--ready-fd",
              strReadyFd.c_str(), NULL);
    }

    // Close virtmgr's FDs.
    serverFd.reset();
    readyFd.reset();

    // Wait for the server to signal its readiness by closing its end of the pipe.
    char buf;
    if (read(waitFd.get(), &buf, sizeof(buf)) < 0) {
        env->ThrowNew(env->FindClass("android/system/virtualmachine/VirtualMachineException"),
                      "Failed to wait for VirtualizationService to be ready");
        return -1;
    }

    return clientFd.release();
}

JNIEXPORT jobject JNICALL android_system_virtualmachine_VirtualizationService_connect(
        JNIEnv* env, [[maybe_unused]] jobject obj, int clientFd) {
    RpcSessionHandle session;
    ARpcSession_setFileDescriptorTransportMode(session.get(),
                                               ARpcSession_FileDescriptorTransportMode::Unix);
    ARpcSession_setMaxIncomingThreads(session.get(), VIRTMGR_THREADS);
    ARpcSession_setMaxOutgoingThreads(session.get(), VIRTMGR_THREADS);
    // SAFETY - ARpcSession_setupUnixDomainBootstrapClient does not take ownership of clientFd.
    auto client = ARpcSession_setupUnixDomainBootstrapClient(session.get(), clientFd);
    return AIBinder_toJavaBinder(env, client);
}

JNIEXPORT jint JNI_OnLoad(JavaVM* vm, void* /*reserved*/) {
    JNIEnv* env;
    if (vm->GetEnv(reinterpret_cast<void**>(&env), JNI_VERSION_1_6) != JNI_OK) {
        ALOGE("%s: Failed to get the environment", __FUNCTION__);
        return JNI_ERR;
    }

    jclass c = env->FindClass("android/system/virtualmachine/VirtualizationService");
    if (c == nullptr) {
        ALOGE("%s: Failed to find class android.system.virtualmachine.VirtualizationService",
              __FUNCTION__);
        return JNI_ERR;
    }

    // Register your class' native methods.
    static const JNINativeMethod methods[] = {
            {"nativeSpawn", "()I",
             reinterpret_cast<void*>(android_system_virtualmachine_VirtualizationService_spawn)},
            {"nativeConnect", "(I)Landroid/os/IBinder;",
             reinterpret_cast<void*>(android_system_virtualmachine_VirtualizationService_connect)},
    };
    int rc = env->RegisterNatives(c, methods, sizeof(methods) / sizeof(JNINativeMethod));
    if (rc != JNI_OK) {
        ALOGE("%s: Failed to register natives", __FUNCTION__);
        return rc;
    }

    return JNI_VERSION_1_6;
}
