#!/bin/bash

set -e

cd "${KOKORO_ARTIFACTS_DIR}/git/avf/build/debian/"
sudo losetup -D
grep vmx /proc/cpuinfo || true
sudo ./build.sh -a x86_64

mkdir -p ${KOKORO_ARTIFACTS_DIR}/logs
sudo cp -r /var/log/fai/* ${KOKORO_ARTIFACTS_DIR}/logs || true
