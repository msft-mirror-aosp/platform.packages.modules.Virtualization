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
import android.content.Intent;
import android.os.Bundle;
import android.util.Log;
import android.view.Menu;
import android.view.MenuItem;
import android.webkit.WebChromeClient;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.TextView;
import android.widget.Toast;

import androidx.appcompat.app.AppCompatActivity;

import com.android.virtualization.vmlauncher.VmLauncherServices;

import com.google.android.material.appbar.MaterialToolbar;

public class MainActivity extends AppCompatActivity implements
        VmLauncherServices.VmLauncherServiceCallback {
    private static final String TAG = "VmTerminalApp";
    private String mVmIpAddr;
    private WebView mWebView;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        Toast.makeText(this, R.string.vm_creation_message, Toast.LENGTH_SHORT).show();
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
                });
    }

    @Override
    protected void onDestroy() {
        VmLauncherServices.stopVmLauncherService(this);
        super.onDestroy();
    }

    private void gotoURL(String url) {
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
        gotoURL("http://" + mVmIpAddr + ":7681");
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
}
