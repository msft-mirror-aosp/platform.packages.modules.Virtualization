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
package com.android.virtualization.vmlauncher;

import android.content.Context;
import android.os.Environment;
import android.util.Log;

import org.apache.commons.compress.archivers.ArchiveEntry;
import org.apache.commons.compress.archivers.tar.TarArchiveInputStream;
import org.apache.commons.compress.compressors.gzip.GzipCompressorInputStream;

import java.io.BufferedInputStream;
import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.HashMap;
import java.util.Map;
import java.util.function.Function;

public class InstallUtils {
    private static final String TAG = InstallUtils.class.getSimpleName();

    private static final String VM_CONFIG_FILENAME = "vm_config.json";
    private static final String COMPRESSED_PAYLOAD_FILENAME = "images.tar.gz";
    private static final String PAYLOAD_DIR = "linux";

    public static String getVmConfigPath(Context context) {
        return new File(context.getFilesDir(), PAYLOAD_DIR)
                .toPath()
                .resolve(VM_CONFIG_FILENAME)
                .toString();
    }

    public static boolean isImageInstalled(Context context) {
        return Files.exists(Path.of(getVmConfigPath(context)));
    }

    private static Path getPayloadPath() {
        File payloadDir = Environment.getExternalStoragePublicDirectory(PAYLOAD_DIR);
        if (payloadDir == null) {
            Log.d(TAG, "no payload dir: " + payloadDir);
            return null;
        }
        Path payloadPath = payloadDir.toPath().resolve(COMPRESSED_PAYLOAD_FILENAME);
        return payloadPath;
    }

    public static boolean payloadFromExternalStorageExists() {
        return Files.exists(getPayloadPath());
    }

    public static boolean installImageFromExternalStorage(Context context) {
        if (!payloadFromExternalStorageExists()) {
            Log.d(TAG, "no artifact file from external storage");
        }
        Path payloadPath = getPayloadPath();
        try (BufferedInputStream inputStream =
                        new BufferedInputStream(Files.newInputStream(payloadPath));
                TarArchiveInputStream tar =
                        new TarArchiveInputStream(new GzipCompressorInputStream(inputStream))) {
            ArchiveEntry entry;
            Path baseDir = new File(context.getFilesDir(), PAYLOAD_DIR).toPath();
            Files.createDirectories(baseDir);
            while ((entry = tar.getNextEntry()) != null) {
                Path extractTo = baseDir.resolve(entry.getName());
                if (entry.isDirectory()) {
                    Files.createDirectories(extractTo);
                } else {
                    Files.copy(tar, extractTo, StandardCopyOption.REPLACE_EXISTING);
                }
            }
        } catch (IOException e) {
            Log.e(TAG, "installation failed", e);
            return false;
        }
        if (!isImageInstalled(context)) {
            return false;
        }

        if (!resolvePathInVmConfig(context)) {
            Log.d(TAG, "resolving path failed");
            try {
                Files.deleteIfExists(Path.of(getVmConfigPath(context)));
            } catch (IOException e) {
                return false;
            }
            return false;
        }

        // remove payload if installation is done.
        try {
            Files.deleteIfExists(payloadPath);
        } catch (IOException e) {
            Log.d(TAG, "failed to remove installed payload", e);
        }

        return true;
    }

    private static Function<String, String> getReplacer(Context context) {
        Map<String, String> rules = new HashMap<>();
        rules.put("\\$PAYLOAD_DIR", new File(context.getFilesDir(), PAYLOAD_DIR).toString());
        return (s) -> {
            for (Map.Entry<String, String> rule : rules.entrySet()) {
                s = s.replaceAll(rule.getKey(), rule.getValue());
            }
            return s;
        };
    }

    private static boolean resolvePathInVmConfig(Context context) {
        try {
            String replacedVmConfig =
                    String.join(
                            "\n",
                            Files.readAllLines(Path.of(getVmConfigPath(context))).stream()
                                    .map(getReplacer(context))
                                    .toList());
            Files.write(Path.of(getVmConfigPath(context)), replacedVmConfig.getBytes());
            return true;
        } catch (IOException e) {
            return false;
        }
    }
}