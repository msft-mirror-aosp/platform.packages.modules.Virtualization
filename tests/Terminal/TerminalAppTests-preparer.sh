#!/bin/bash

# Copyright 2024 Google Inc. All rights reserved.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

set -e

serial=${ANDROID_SERIAL}

# Identify file to download
arch=$(adb -s ${serial} shell getprop ro.bionic.arch)
if [ ${arch} == "arm64" ]; then
  src=https://github.com/ikicha/debian_ci/releases/download/release_aarch64/images.tar.gz
else
  src=https://github.com/ikicha/debian_ci/releases/download/release_x86_64/images.tar.gz
fi

# Download
downloaded=$(tempfile)
wget ${src} -O ${downloaded}

# Push the file to the device
user=$(adb -s ${serial} shell am get-current-user)
dst=/data/media/${user}/linux
adb -s ${serial} shell mkdir -p ${dst}
adb -s ${serial} push ${downloaded} ${dst}/images.tar.gz
