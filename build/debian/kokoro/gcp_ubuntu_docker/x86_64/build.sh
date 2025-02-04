#!/bin/bash

set -e

# pyenv set up to use python 3.11
sudo apt update; sudo apt install build-essential libssl-dev zlib1g-dev \
  libbz2-dev libreadline-dev libsqlite3-dev curl git \
  libncursesw5-dev xz-utils tk-dev libxml2-dev libxmlsec1-dev libffi-dev liblzma-dev
git clone https://github.com/pyenv/pyenv.git ~/.pyenv
export PYENV_ROOT="$HOME/.pyenv"
export PATH="$PYENV_ROOT/bin:$PATH"
pyenv install 3.11
pyenv global 3.11
python --version

cd "${KOKORO_ARTIFACTS_DIR}/git/avf/build/debian/"
sudo losetup -D
grep vmx /proc/cpuinfo || true
sudo ./build.sh -a x86_64 -k -r
sudo mv images.tar.gz ${KOKORO_ARTIFACTS_DIR} || true
mkdir -p ${KOKORO_ARTIFACTS_DIR}/logs
sudo cp -r /var/log/fai/* ${KOKORO_ARTIFACTS_DIR}/logs || true
