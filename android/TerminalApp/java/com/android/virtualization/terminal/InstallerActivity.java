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

import android.annotation.MainThread;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.os.Build;
import android.os.Bundle;
import android.os.ConditionVariable;
import android.os.FileUtils;
import android.os.IBinder;
import android.os.RemoteException;
import android.text.format.Formatter;
import android.util.Log;
import android.view.View;
import android.widget.CheckBox;
import android.widget.TextView;

import com.android.internal.annotations.VisibleForTesting;

import com.google.android.material.progressindicator.LinearProgressIndicator;
import com.google.android.material.snackbar.Snackbar;

import java.lang.ref.WeakReference;
import java.util.concurrent.ExecutorService;

public class InstallerActivity extends BaseActivity {
    private static final String TAG = "LinuxInstaller";

    private static final long ESTIMATED_IMG_SIZE_BYTES = FileUtils.parseSize("350MB");
    static final String EXTRA_AUTO_DOWNLOAD = "auto_download";

    private ExecutorService mExecutorService;
    private CheckBox mWaitForWifiCheckbox;
    private TextView mInstallButton;

    private IInstallerService mService;
    private ServiceConnection mInstallerServiceConnection;
    private InstallProgressListener mInstallProgressListener;
    private boolean mInstallRequested;
    private ConditionVariable mInstallCompleted = new ConditionVariable();

    @Override
    public void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setResult(RESULT_CANCELED);

        mInstallProgressListener = new InstallProgressListener(this);

        setContentView(R.layout.activity_installer);

        TextView desc = (TextView) findViewById(R.id.installer_desc);
        desc.setText(
                getString(
                        R.string.installer_desc_text_format,
                        Formatter.formatShortFileSize(this, ESTIMATED_IMG_SIZE_BYTES)));

        mWaitForWifiCheckbox = (CheckBox) findViewById(R.id.installer_wait_for_wifi_checkbox);
        mInstallButton = (TextView) findViewById(R.id.installer_install_button);

        mInstallButton.setOnClickListener(
                (event) -> {
                    requestInstall();
                });

        if (getIntent().getBooleanExtra(EXTRA_AUTO_DOWNLOAD, false)) {
            Log.i(TAG, "Auto downloading");
            requestInstall();
        }

        Intent intent = new Intent(this, InstallerService.class);
        mInstallerServiceConnection = new InstallerServiceConnection(this);
        if (!bindService(intent, mInstallerServiceConnection, Context.BIND_AUTO_CREATE)) {
            handleCriticalError(new Exception("Failed to connect to installer service"));
        }

    }

    @Override
    public void onDestroy() {
        if (mInstallerServiceConnection != null) {
            unbindService(mInstallerServiceConnection);
            mInstallerServiceConnection = null;
        }

        super.onDestroy();
    }

    @VisibleForTesting
    public boolean waitForInstallCompleted(long timeoutMillis) {
        return mInstallCompleted.block(timeoutMillis);
    }

    public void handleCriticalError(Exception e) {
        if (Build.isDebuggable()) {
            Snackbar.make(
                            findViewById(android.R.id.content),
                            e.getMessage() + ". File a bugreport to go/ferrochrome-bug",
                            Snackbar.LENGTH_INDEFINITE)
                    .show();
        }
        Log.e(TAG, "Internal error", e);
        finishWithResult(RESULT_CANCELED);
    }

    private void finishWithResult(int resultCode) {
        if (resultCode == RESULT_OK) {
            mInstallCompleted.open();
        }
        setResult(resultCode);
        finish();
    }

    private void setInstallEnabled(boolean enable) {
        mInstallButton.setEnabled(enable);
        mWaitForWifiCheckbox.setEnabled(enable);
        LinearProgressIndicator progressBar = findViewById(R.id.installer_progress);
        if (enable) {
            progressBar.setVisibility(View.INVISIBLE);
        } else {
            progressBar.setVisibility(View.VISIBLE);
        }

        int resId =
                enable
                        ? R.string.installer_install_button_enabled_text
                        : R.string.installer_install_button_disabled_text;
        mInstallButton.setText(getString(resId));
    }

    @MainThread
    private void requestInstall() {
        setInstallEnabled(/* enable= */ false);

        if (mService != null) {
            try {
                mService.requestInstall();
            } catch (RemoteException e) {
                handleCriticalError(e);
            }
        } else {
            Log.d(TAG, "requestInstall() is called, but not yet connected");
            mInstallRequested = true;
        }
    }

    @MainThread
    public void handleInstallerServiceConnected() {
        try {
            mService.setProgressListener(mInstallProgressListener);
            if (mService.isInstalled()) {
                // Finishing this activity will trigger MainActivity::onResume(),
                // and VM will be started from there.
                finishWithResult(RESULT_OK);
                return;
            }

            if (mInstallRequested) {
                requestInstall();
            } else if (mService.isInstalling()) {
                setInstallEnabled(false);
            }
        } catch (RemoteException e) {
            handleCriticalError(e);
        }
    }

    @MainThread
    public void handleInstallerServiceDisconnected() {
        handleCriticalError(new Exception("InstallerService is destroyed while in use"));
    }

    @MainThread
    private void handleError(String displayText) {
        if (Build.isDebuggable()) {
            Snackbar.make(findViewById(android.R.id.content), displayText, Snackbar.LENGTH_LONG)
                    .show();
        }
        setInstallEnabled(true);
    }

    private static class InstallProgressListener extends IInstallProgressListener.Stub {
        private final WeakReference<InstallerActivity> mActivity;

        InstallProgressListener(InstallerActivity activity) {
            mActivity = new WeakReference<>(activity);
        }

        @Override
        public void onCompleted() {
            InstallerActivity activity = mActivity.get();
            if (activity == null) {
                // Ignore incoming connection or disconnection after activity is destroyed.
                return;
            }

            // MainActivity will be resume and handle rest of progress.
            activity.finishWithResult(RESULT_OK);
        }

        @Override
        public void onError(String displayText) {
            InstallerActivity context = mActivity.get();
            if (context == null) {
                // Ignore incoming connection or disconnection after activity is destroyed.
                return;
            }

            context.runOnUiThread(
                    () -> {
                        InstallerActivity activity = mActivity.get();
                        if (activity == null) {
                            // Ignore incoming connection or disconnection after activity is
                            // destroyed.
                            return;
                        }

                        activity.handleError(displayText);
                    });
        }
    }

    @MainThread
    public static final class InstallerServiceConnection implements ServiceConnection {
        private final WeakReference<InstallerActivity> mActivity;

        InstallerServiceConnection(InstallerActivity activity) {
            mActivity = new WeakReference<>(activity);
        }

        @Override
        public void onServiceConnected(ComponentName name, IBinder service) {
            InstallerActivity activity = mActivity.get();
            if (activity == null || activity.mInstallerServiceConnection == null) {
                // Ignore incoming connection or disconnection after activity is destroyed.
                return;
            }
            if (service == null) {
                activity.handleCriticalError(new Exception("service shouldn't be null"));
            }

            activity.mService = IInstallerService.Stub.asInterface(service);
            activity.handleInstallerServiceConnected();
        }

        @Override
        public void onServiceDisconnected(ComponentName name) {
            InstallerActivity activity = mActivity.get();
            if (activity == null || activity.mInstallerServiceConnection == null) {
                // Ignore incoming connection or disconnection after activity is destroyed.
                return;
            }

            if (activity.mInstallerServiceConnection != null) {
                activity.unbindService(activity.mInstallerServiceConnection);
                activity.mInstallerServiceConnection = null;
            }
            activity.handleInstallerServiceDisconnected();
        }
    }
}
