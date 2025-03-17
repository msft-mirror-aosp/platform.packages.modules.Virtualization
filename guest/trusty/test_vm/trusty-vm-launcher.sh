#!/bin/sh

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

mkdir -p /data/local/tmp/trusty_test_vm/logs || true
/apex/com.android.virt/bin/vm run \
   --console /data/local/tmp/trusty_test_vm/logs/console.log \
   /data/local/tmp/trusty_test_vm/trusty-test_vm-config.json
