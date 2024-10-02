#!/bin/bash

set -e

cd "${KOKORO_ARTIFACTS_DIR}/git/avf/build/debian/"
sudo losetup -D
grep vmx /proc/cpuinfo || true
sudo ./build.sh
cp image.raw ${KOKORO_ARTIFACTS_DIR}
