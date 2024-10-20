#!/bin/bash

set -e

cd "${KOKORO_ARTIFACTS_DIR}/git/avf/build/debian/"
sudo losetup -D
grep vmx /proc/cpuinfo || true
sudo ./build.sh
tar czvS -f ${KOKORO_ARTIFACTS_DIR}/images.tar.gz image.raw vm_config.json.aarch64 --transform s/vm_config.json.aarch64/vm_config.json/

mkdir -p ${KOKORO_ARTIFACTS_DIR}/logs
sudo cp -r /var/log/fai/* ${KOKORO_ARTIFACTS_DIR}/logs || true
