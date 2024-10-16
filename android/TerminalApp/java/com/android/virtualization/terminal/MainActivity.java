/*
 * Copyright (C) 2024 The Android Open Source Project
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

import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.os.Bundle;
import android.system.ErrnoException;
import android.system.Os;
import android.util.Log;
import android.view.Menu;
import android.view.MenuItem;
import android.view.accessibility.AccessibilityManager;
import android.webkit.WebChromeClient;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.TextView;
import android.widget.Toast;

import androidx.appcompat.app.AppCompatActivity;

import com.android.virtualization.vmlauncher.VmLauncherServices;

import com.google.android.material.appbar.MaterialToolbar;

import java.io.File;
import java.io.FileDescriptor;
import java.io.FileNotFoundException;
import java.io.IOException;
import java.io.RandomAccessFile;

public class MainActivity extends AppCompatActivity
        implements VmLauncherServices.VmLauncherServiceCallback,
                AccessibilityManager.TouchExplorationStateChangeListener {
    private static final String TAG = "VmTerminalApp";
    private String mVmIpAddr;
    private WebView mWebView;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        try {
            // No resize for now.
            long newSizeInBytes = 0;
            diskResize(this, newSizeInBytes);
        } catch (IOException e) {
            Log.e(TAG, "Failed to resize disk", e);
            Toast.makeText(this, "Error resizing disk: " + e.getMessage(), Toast.LENGTH_LONG)
                    .show();
        }

        Toast.makeText(this, R.string.vm_creation_message, Toast.LENGTH_SHORT).show();
        android.os.Trace.beginAsyncSection("executeTerminal", 0);
        VmLauncherServices.startVmLauncherService(this, this);

        setContentView(R.layout.activity_headless);

        MaterialToolbar toolbar = (MaterialToolbar) findViewById(R.id.toolbar);
        setSupportActionBar(toolbar);
        mWebView = (WebView) findViewById(R.id.webview);
        mWebView.getSettings().setDatabaseEnabled(true);
        mWebView.getSettings().setDomStorageEnabled(true);
        mWebView.getSettings().setJavaScriptEnabled(true);
        mWebView.setWebChromeClient(new WebChromeClient());
        mWebView.setWebViewClient(
                new WebViewClient() {
                    @Override
                    public boolean shouldOverrideUrlLoading(WebView view, String url) {
                        view.loadUrl(url);
                        return true;
                    }

                    @Override
                    public void onPageFinished(WebView view, String url) {
                        android.os.Trace.endAsyncSection("executeTerminal", 0);
                    }
                });

        getSystemService(AccessibilityManager.class).addTouchExplorationStateChangeListener(this);
    }

    private void diskResize(Context context, long sizeInBytes) throws IOException {
        try {
            if (sizeInBytes == 0) {
                return;
            }
            File file = getPartitionFile(context, "root_part");
            String filePath = file.getAbsolutePath();
            Log.d(TAG, "Disk-resize in progress for partition: " + filePath);

            long currentSize = Os.stat(filePath).st_size;
            runE2fsck(filePath);
            if (sizeInBytes > currentSize) {
                allocateSpace(file, sizeInBytes);
            }

            resizeFilesystem(filePath, sizeInBytes);
        } catch (ErrnoException e) {
            Log.e(TAG, "ErrnoException during disk resize", e);
            throw new IOException("ErrnoException during disk resize", e);
        } catch (IOException e) {
            Log.e(TAG, "Failed to resize disk", e);
            throw e;
        }
    }

    private static File getPartitionFile(Context context, String fileName)
            throws FileNotFoundException {
        File file = new File(context.getFilesDir(), fileName);
        if (!file.exists()) {
            Log.d(TAG, fileName + " - file not found");
            throw new FileNotFoundException("File not found: " + fileName);
        }
        return file;
    }

    private static void allocateSpace(File file, long sizeInBytes) throws IOException {
        try {
            RandomAccessFile raf = new RandomAccessFile(file, "rw");
            FileDescriptor fd = raf.getFD();
            Os.posix_fallocate(fd, 0, sizeInBytes);
            raf.close();
            Log.d(TAG, "Allocated space to: " + sizeInBytes + " bytes");
        } catch (ErrnoException e) {
            Log.e(TAG, "Failed to allocate space", e);
            throw new IOException("Failed to allocate space", e);
        }
    }

    private static void runE2fsck(String filePath) throws IOException {
        try {
            runCommand("/system/bin/e2fsck", "-f", filePath);
            Log.d(TAG, "e2fsck completed: " + filePath);
        } catch (IOException e) {
            Log.e(TAG, "Failed to run e2fsck", e);
            throw e;
        }
    }

    private static void resizeFilesystem(String filePath, long sizeInBytes) throws IOException {
        long sizeInMB = sizeInBytes / (1024 * 1024);
        if (sizeInMB == 0) {
            Log.e(TAG, "Invalid size: " + sizeInBytes + " bytes");
            throw new IllegalArgumentException("Size cannot be zero MB");
        }
        String sizeArg = sizeInMB + "M";
        try {
            runCommand("/system/bin/resize2fs", filePath, sizeArg);
            Log.d(TAG, "resize2fs completed: " + filePath + ", size: " + sizeArg);
        } catch (IOException e) {
            Log.e(TAG, "Failed to run resize2fs", e);
            throw e;
        }
    }

    private static void runCommand(String... command) throws IOException {
        try {
            Process process = new ProcessBuilder(command).redirectErrorStream(true).start();
            process.waitFor();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new IOException("Command interrupted", e);
        }
    }

    @Override
    protected void onDestroy() {
        getSystemService(AccessibilityManager.class).removeTouchExplorationStateChangeListener(this);
        VmLauncherServices.stopVmLauncherService(this);
        super.onDestroy();
    }

    private void gotoTerminalURL() {
        if (mVmIpAddr == null) {
            Log.d(TAG, "ip addr is not set yet");
            return;
        }

        boolean isTouchExplorationEnabled =
                getSystemService(AccessibilityManager.class).isTouchExplorationEnabled();

        String url =
                "http://"
                        + mVmIpAddr
                        + ":7681/"
                        + (isTouchExplorationEnabled ? "?screenReaderMode=true" : "");
        runOnUiThread(() -> mWebView.loadUrl(url));
    }

    @Override
    public void onVmStart() {
        Log.i(TAG, "onVmStart()");
    }

    @Override
    public void onVmStop() {
        Toast.makeText(this, R.string.vm_stop_message, Toast.LENGTH_SHORT).show();
        Log.i(TAG, "onVmStop()");
        finish();
    }

    @Override
    public void onVmError() {
        Toast.makeText(this, R.string.vm_error_message, Toast.LENGTH_SHORT).show();
        Log.i(TAG, "onVmError()");
        finish();
    }

    @Override
    public void onIpAddrAvailable(String ipAddr) {
        mVmIpAddr = ipAddr;
        ((TextView) findViewById(R.id.ip_addr_textview)).setText(mVmIpAddr);
        gotoTerminalURL();
    }

    @Override
    public boolean onCreateOptionsMenu(Menu menu) {
        getMenuInflater().inflate(R.menu.main_menu, menu);
        return true;
    }

    @Override
    public boolean onOptionsItemSelected(MenuItem item) {
        int id = item.getItemId();
        if (id == R.id.copy_ip_addr) {
            // TODO(b/340126051): remove this menu item when port forwarding is supported.
            getSystemService(ClipboardManager.class)
                    .setPrimaryClip(ClipData.newPlainText("A VM's IP address", mVmIpAddr));
            return true;
        } else if (id == R.id.stop_vm) {
            VmLauncherServices.stopVmLauncherService(this);
            return true;

        } else if (id == R.id.menu_item_settings) {
            Intent intent = new Intent(this, SettingsActivity.class);
            this.startActivity(intent);
            return true;
        }
        return super.onOptionsItemSelected(item);
    }

    @Override
    public void onTouchExplorationStateChanged(boolean enabled) {
        gotoTerminalURL();
    }
}
