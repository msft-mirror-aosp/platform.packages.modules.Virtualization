#!/bin/bash

if [ -z $ANDROID_BUILD_TOP ]; then echo "forgot to source build/envsetup.sh?" && exit 1; fi

docker run --privileged -it -v $ANDROID_BUILD_TOP/packages/modules/Virtualization:/root/Virtualization -v /dev:/dev ubuntu:22.04 /root/Virtualization/build/debian/build.sh
