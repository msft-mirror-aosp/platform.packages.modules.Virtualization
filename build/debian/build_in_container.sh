#!/bin/bash

show_help() {
  echo "Usage: sudo $0 [OPTION]..."
  echo "Builds a debian image and save it to image.raw."
  echo "Options:"
  echo "-h         Print usage and this help message and exit."
  echo "-a ARCH    Architecture of the image [default is host arch: $(uname -m)]"
  echo "-g         Use Debian generic kernel [default is our custom kernel]"
  echo "-r         Release mode build"
  echo "-s         Leave a shell open [default: only if the build fails]"
  echo "-u         Set VM boot mode to u-boot [default is to load kernel directly]"
  echo "-w         Save temp work directory in the container [for debugging]"
}

arch="$(uname -m)"
kernel_flag=
release_flag=
save_workdir_flag=
shell_condition="||"
uboot_flag=

while getopts "a:ghrsuw" option; do
  case ${option} in
    a)
      arch="$OPTARG"
      ;;
    g)
      kernel_flag="-g"
      ;;
    h)
      show_help ; exit
      ;;
    r)
      release_flag="-r"
      ;;
    s)
      shell_condition=";"
      ;;
    u)
      uboot_flag="-u"
      ;;
    w)
      save_workdir_flag="-w"
      ;;
    *)
      echo "Invalid option: $OPTARG" ; exit 1
      ;;
  esac
done

if [[ "$arch" != "aarch64" && "$arch" != "x86_64" ]]; then
  echo "Invalid architecture: $arch" ; exit 1
fi

if [ -z "$ANDROID_BUILD_TOP" ] ; then
  echo '`ANDROID_BUILD_TOP` is undefined.'
  echo 'Please `lunch` an Android target, or manually set the variable.'
  exit 1
fi

docker run --privileged -it -v /dev:/dev \
  -v "$ANDROID_BUILD_TOP/packages/modules/Virtualization:/root/Virtualization" \
  --workdir /root/Virtualization/build/debian \
  ubuntu:22.04 \
  bash -c "./build.sh -a $arch $release_flag $kernel_flag $uboot_flag $save_workdir_flag $shell_condition bash"
