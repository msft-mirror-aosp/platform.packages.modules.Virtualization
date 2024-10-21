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

package com.android.virtualization.terminal;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.os.Build;
import android.os.IBinder;
import android.util.Log;

import androidx.annotation.Nullable;

import com.android.internal.annotations.GuardedBy;
import com.android.virtualization.vmlauncher.InstallUtils;

import java.lang.ref.WeakReference;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class InstallerService extends Service {
    private static final String TAG = "InstallerService";

    private static final String NOTIFICATION_CHANNEL_ID = "installer";
    private static final int NOTIFICATION_ID = 1313; // any unique number among notifications

    private final Object mLock = new Object();

    private Notification mNotification;

    @GuardedBy("mLock")
    private boolean mIsInstalling;

    @GuardedBy("mLock")
    private IInstallProgressListener mListener;

    private ExecutorService mExecutorService;

    @Override
    public void onCreate() {
        super.onCreate();

        // Create mandatory notification
        NotificationManager manager = getSystemService(NotificationManager.class);
        if (manager.getNotificationChannel(NOTIFICATION_CHANNEL_ID) == null) {
            NotificationChannel channel =
                    new NotificationChannel(
                            NOTIFICATION_CHANNEL_ID,
                            getString(R.string.installer_notif_title_text),
                            NotificationManager.IMPORTANCE_DEFAULT);
            manager.createNotificationChannel(channel);
        }

        Intent intent = new Intent(this, MainActivity.class);
        PendingIntent pendingIntent =
                PendingIntent.getActivity(
                        this, /* requestCode= */ 0, intent, PendingIntent.FLAG_IMMUTABLE);
        mNotification =
                new Notification.Builder(this, NOTIFICATION_CHANNEL_ID)
                        .setSmallIcon(R.drawable.ic_launcher_foreground)
                        .setContentTitle(getString(R.string.installer_notif_title_text))
                        .setContentText(getString(R.string.installer_notif_desc_text))
                        .setOngoing(true)
                        .setContentIntent(pendingIntent)
                        .build();

        mExecutorService = Executors.newSingleThreadExecutor();
    }

    @Nullable
    @Override
    public IBinder onBind(Intent intent) {
        return new InstallerServiceImpl(this);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        super.onStartCommand(intent, flags, startId);

        Log.d(TAG, "Starting service ...");

        return START_STICKY;
    }

    @Override
    public void onDestroy() {
        super.onDestroy();

        Log.d(TAG, "Service is destroyed");
        if (mExecutorService != null) {
            mExecutorService.shutdown();
        }
    }

    private void requestInstall() {
        Log.i(TAG, "Installing..");

        // Make service to be long running, even after unbind() when InstallerActivity is destroyed
        // The service will still be destroyed if task is remove.
        startService(new Intent(this, InstallerService.class));
        startForeground(
                NOTIFICATION_ID, mNotification, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE);
        synchronized (mLock) {
            mIsInstalling = true;
        }

        mExecutorService.execute(
                () -> {
                    Log.d(TAG, "installLinuxImage");

                    // Installing from sdcard is preferred, but only supported only in debuggable
                    // build.
                    if (Build.isDebuggable()) {
                        Log.i(TAG, "trying to install /sdcard/linux/images.tar.gz");

                        // TODO(b/374015561): Provide progress update
                        if (InstallUtils.installImageFromExternalStorage(this)) {
                            Log.i(TAG, "image is installed from /sdcard/linux/images.tar.gz");
                        } else {
                            // TODO(b/374015561): Notify error
                            Log.e(TAG, "Install failed");
                        }
                    }
                    stopForeground(STOP_FOREGROUND_REMOVE);

                    synchronized (mLock) {
                        mIsInstalling = false;
                    }
                    notifyCompleted();
                });
    }

    private void notifyCompleted() {
        IInstallProgressListener listener;
        synchronized (mLock) {
            listener = mListener;
        }

        try {
            listener.onCompleted();
        } catch (Exception e) {
            // ignore. Activity may not exist.
        }
    }

    private static final class InstallerServiceImpl extends IInstallerService.Stub {
        // Holds weak reference to avoid Context leak
        private final WeakReference<InstallerService> mService;

        public InstallerServiceImpl(InstallerService service) {
            mService = new WeakReference<>(service);
        }

        private InstallerService ensureServiceConnected() throws RuntimeException {
            InstallerService service = mService.get();
            if (service == null) {
                throw new RuntimeException(
                        "Internal error: Installer service is being accessed after destroyed");
            }
            return service;
        }

        @Override
        public void requestInstall() {
            InstallerService service = ensureServiceConnected();
            synchronized (service.mLock) {
                service.requestInstall();
            }
        }

        @Override
        public void setProgressListener(IInstallProgressListener listener) {
            InstallerService service = ensureServiceConnected();
            synchronized (service.mLock) {
                service.mListener = listener;
            }
        }

        @Override
        public boolean isInstalling() {
            InstallerService service = ensureServiceConnected();
            synchronized (service.mLock) {
                return service.mIsInstalling;
            }
        }

        @Override
        public boolean isInstalled() {
            InstallerService service = ensureServiceConnected();
            synchronized (service.mLock) {
                return !service.mIsInstalling && InstallUtils.isImageInstalled(service);
            }
        }
    }
}
