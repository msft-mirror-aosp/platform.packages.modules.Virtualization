#!/bin/bash
set -e

if [ "$#" -ne 1 ]; then
    echo "$0 <image.raw path>"
    exit 1
fi
pushd $(dirname $0) > /dev/null
tempdir=$(mktemp -d)
asset_dir=./assets/linux
mkdir -p ${asset_dir}
echo Copy files...
pushd ${tempdir} > /dev/null
cp "$1" ${tempdir}
tar czvS -f images.tar.gz $(basename $1)
popd > /dev/null
cp vm_config.json ${asset_dir}
mv ${tempdir}/images.tar.gz ${asset_dir}
echo Calculating hash...
hash=$(cat ${asset_dir}/images.tar.gz ${asset_dir}/vm_config.json | sha1sum | cut -d' ' -f 1)
echo ${hash} > ${asset_dir}/hash
popd > /dev/null
echo Cleaning up...
rm -rf ${tempdir}

