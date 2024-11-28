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
package com.android.virtualization.terminal

import android.content.Context
import android.content.SharedPreferences
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Window
import android.view.WindowManager
import androidx.appcompat.app.AppCompatActivity
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView

class SettingsPortForwardingActivity : AppCompatActivity() {
    private lateinit var mSharedPref: SharedPreferences
    private lateinit var mAdapter: SettingsPortForwardingAdapter

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.settings_port_forwarding)

        Handler(Looper.getMainLooper()).post {
            val lp: WindowManager.LayoutParams = getWindow().getAttributes()
            lp.accessibilityTitle = getString(R.string.settings_port_forwarding_title)
            getWindow().setAttributes(lp)
        }

        mSharedPref =
            this.getSharedPreferences(getString(R.string.preference_file_key), Context.MODE_PRIVATE)

        mAdapter = SettingsPortForwardingAdapter(mSharedPref, this)

        val recyclerView: RecyclerView = findViewById(R.id.settings_port_forwarding_recycler_view)
        recyclerView.layoutManager = LinearLayoutManager(this)
        recyclerView.adapter = mAdapter
    }

    override fun onResume() {
        super.onResume()
        mSharedPref.registerOnSharedPreferenceChangeListener(mAdapter)
    }

    override fun onPause() {
        mSharedPref.unregisterOnSharedPreferenceChangeListener(mAdapter)
        super.onPause()
    }
}
