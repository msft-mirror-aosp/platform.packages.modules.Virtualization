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
import android.content.Intent
import android.icu.text.MeasureFormat
import android.icu.text.NumberFormat
import android.icu.util.Measure
import android.icu.util.MeasureUnit
import android.os.Bundle
import android.text.SpannableString
import android.text.Spanned
import android.text.TextUtils
import android.text.style.RelativeSizeSpan
import android.widget.SeekBar
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.isVisible
import com.google.android.material.button.MaterialButton
import java.util.Locale
import java.util.regex.Pattern

class SettingsDiskResizeActivity : AppCompatActivity() {
    private val maxDiskSizeMb: Long = 16 shl 10
    private val numberPattern: Pattern = Pattern.compile("[\\d]*[\\٫.,]?[\\d]+")

    private var diskSizeStepMb: Long = 0
    private var diskSizeMb: Long = 0
    private lateinit var diskSizeText: TextView
    private lateinit var diskSizeSlider: SeekBar

    private fun bytesToMb(bytes: Long): Long {
        return bytes shr 20
    }

    private fun mbToBytes(bytes: Long): Long {
        return bytes shl 20
    }

    private fun mbToProgress(bytes: Long): Int {
        return (bytes / diskSizeStepMb).toInt()
    }

    private fun progressToMb(progress: Int): Long {
        return progress * diskSizeStepMb
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.settings_disk_resize)

        diskSizeStepMb = 1L shl resources.getInteger(R.integer.disk_size_round_up_step_size_in_mb)

        val sharedPref =
            this.getSharedPreferences(getString(R.string.preference_file_key), Context.MODE_PRIVATE)
        diskSizeMb =
            bytesToMb(
                sharedPref.getLong(getString(R.string.preference_disk_size_key), /* defValue= */ 0)
            )
        val image = InstalledImage.getDefault(this)
        val minDiskSizeMb = bytesToMb(image.getSmallestSizePossible()).coerceAtMost(diskSizeMb)

        diskSizeText = findViewById<TextView>(R.id.settings_disk_resize_resize_gb_assigned)!!
        val diskMaxSizeText = findViewById<TextView>(R.id.settings_disk_resize_resize_gb_max)
        diskMaxSizeText.text =
            getString(
                R.string.settings_disk_resize_resize_gb_max_format,
                localizedFileSize(maxDiskSizeMb, /* isShort= */ true),
            )

        diskSizeSlider = findViewById<SeekBar>(R.id.settings_disk_resize_disk_size_slider)!!
        val cancelButton = findViewById<MaterialButton>(R.id.settings_disk_resize_cancel_button)
        val resizeButton = findViewById<MaterialButton>(R.id.settings_disk_resize_resize_button)
        diskSizeSlider.min = mbToProgress(minDiskSizeMb)
        diskSizeSlider.max = mbToProgress(maxDiskSizeMb)
        diskSizeSlider.progress = mbToProgress(diskSizeMb)
        updateSliderText(diskSizeMb)

        diskSizeSlider.setOnSeekBarChangeListener(
            object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(seekBar: SeekBar, progress: Int, fromUser: Boolean) {
                    updateSliderText(progressToMb(progress))
                    cancelButton.isVisible = true
                    resizeButton.isVisible = true
                }

                override fun onStartTrackingTouch(seekBar: SeekBar?) {
                    // no-op
                }

                override fun onStopTrackingTouch(seekBar: SeekBar?) {
                    // no-op
                }
            }
        )

        cancelButton.setOnClickListener {
            diskSizeSlider.progress = mbToProgress(diskSizeMb)
            cancelButton.isVisible = false
            resizeButton.isVisible = false
        }

        resizeButton.setOnClickListener {
            diskSizeMb = progressToMb(diskSizeSlider.progress)
            cancelButton.isVisible = false
            resizeButton.isVisible = false
            val editor = sharedPref.edit()
            editor.putLong(getString(R.string.preference_disk_size_key), mbToBytes(diskSizeMb))
            editor.apply()

            // Restart terminal
            val intent =
                baseContext.packageManager.getLaunchIntentForPackage(baseContext.packageName)
            intent?.addFlags(Intent.FLAG_ACTIVITY_CLEAR_TASK)
            finish()
            startActivity(intent)
        }
    }

    fun updateSliderText(sizeMb: Long) {
        diskSizeText.text =
            enlargeFontOfNumber(
                getString(
                    R.string.settings_disk_resize_resize_gb_assigned_format,
                    localizedFileSize(sizeMb, /* isShort= */ true),
                )
            )
        diskSizeSlider.stateDescription =
            getString(
                R.string.settings_disk_resize_resize_gb_assigned_format,
                localizedFileSize(sizeMb, /* isShort= */ false),
            )
    }

    fun localizedFileSize(sizeMb: Long, isShort: Boolean): String {
        val sizeGb = sizeMb / (1 shl 10).toFloat()
        val measure = Measure(sizeGb, MeasureUnit.GIGABYTE)

        val localeFromContext: Locale = resources.configuration.locales[0]
        val numberFormatter: NumberFormat = NumberFormat.getInstance(localeFromContext)
        numberFormatter.minimumFractionDigits = 1
        numberFormatter.maximumFractionDigits = 1

        val formatWidth =
            if (isShort) MeasureFormat.FormatWidth.SHORT else MeasureFormat.FormatWidth.WIDE
        val measureFormat: MeasureFormat =
            MeasureFormat.getInstance(localeFromContext, formatWidth, numberFormatter)
        return measureFormat.format(measure)
    }

    fun enlargeFontOfNumber(summary: CharSequence): CharSequence {
        if (TextUtils.isEmpty(summary)) {
            return ""
        }

        val matcher = numberPattern.matcher(summary)
        if (matcher.find()) {
            val spannableSummary = SpannableString(summary)
            spannableSummary.setSpan(
                RelativeSizeSpan(2f),
                matcher.start(),
                matcher.end(),
                Spanned.SPAN_EXCLUSIVE_EXCLUSIVE,
            )
            return spannableSummary
        }
        return summary
    }
}
