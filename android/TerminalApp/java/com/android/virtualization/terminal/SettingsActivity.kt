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

import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.appbar.MaterialToolbar

class SettingsActivity : AppCompatActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.settings_activity)

        val toolbar: MaterialToolbar = findViewById(R.id.settings_toolbar)
        setSupportActionBar(toolbar)
        val settingsItems =
            arrayOf(
                SettingsItem(
                    resources.getString(R.string.settings_disk_resize_title),
                    resources.getString(R.string.settings_disk_resize_sub_title),
                    R.drawable.baseline_storage_24,
                    SettingsItemEnum.DiskResize,
                ),
                SettingsItem(
                    resources.getString(R.string.settings_port_forwarding_title),
                    resources.getString(R.string.settings_port_forwarding_sub_title),
                    R.drawable.baseline_call_missed_outgoing_24,
                    SettingsItemEnum.PortForwarding,
                ),
                SettingsItem(
                    resources.getString(R.string.settings_recovery_title),
                    resources.getString(R.string.settings_recovery_sub_title),
                    R.drawable.baseline_settings_backup_restore_24,
                    SettingsItemEnum.Recovery,
                ),
            )
        val settingsListItemAdapter = SettingsItemAdapter(settingsItems)

        val recyclerView: RecyclerView = findViewById(R.id.settings_list_recycler_view)
        recyclerView.layoutManager = LinearLayoutManager(this)
        recyclerView.adapter = settingsListItemAdapter
    }
}
