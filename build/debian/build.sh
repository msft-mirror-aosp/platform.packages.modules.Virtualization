#!/bin/bash

# This is a script to build a Debian image that can run in a VM created via AVF.
# TODOs:
# - Support x86_64 architecture
# - Add Android-specific packages via a new class
# - Use a stable release from debian-cloud-images

show_help() {
	echo Usage: $0 [OPTION]... [FILE]
	echo Builds a debian image and save it to FILE.
	echo Options:
	echo -h         Pring usage and this help message and exit.
}

check_sudo() {
	if [ "$EUID" -ne 0 ]; then
		echo "Please run as root."
		exit
	fi
}

parse_options() {
	while getopts ":h" option; do
		case ${option} in
			h)
				show_help
				exit;;
		esac
	done
	if [ -n "$1" ]; then
		built_image=$1
	fi
}

install_prerequisites() {
	apt update
	DEBIAN_FRONTEND=noninteractive \
	apt install --no-install-recommends --assume-yes \
		binfmt-support \
		build-essential \
		ca-certificates \
		curl \
		debsums \
		dosfstools \
		fai-server \
		fai-setup-storage \
		fdisk \
		gcc-aarch64-linux-gnu \
		libc6-dev-arm64-cross \
		make \
		python3 \
		python3-libcloud \
		python3-marshmallow \
		python3-pytest \
		python3-yaml \
		qemu-system-arm \
		qemu-user-static \
		qemu-utils \
		udev \


	if [ ! -f $HOME/.cargo/bin/cargo ]; then
		curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
	fi

        source $HOME/.cargo/env
        rustup target add aarch64-unknown-linux-gnu

        sed -i s/losetup\ -f/losetup\ -P\ -f/g /usr/sbin/fai-diskimage
        sed -i 's/wget \$/wget -t 0 \$/g' /usr/share/debootstrap/functions

        apt install --no-install-recommends --assume-yes curl
        # just for testing
        echo libseccomp: $(curl -is https://deb.debian.org/debian/pool/main/libs/libseccomp/libseccomp2_2.5.4-1+deb12u1_arm64.deb | head -n 1)
        echo libsemanage-common: $(curl -is https://deb.debian.org/debian/pool/main/libs/libsemanage/libsemanage-common_3.4-1_all.deb | head -n 1)
        echo libpcre2: $(curl -is https://deb.debian.org/debian/pool/main/p/pcre2/libpcre2-8-0_10.42-1_arm64.deb | head -n 1)
}

download_debian_cloud_image() {
	local ver=master
	local prj=debian-cloud-images
	local url=https://salsa.debian.org/cloud-team/${prj}/-/archive/${ver}/${prj}-${ver}.tar.gz
	local outdir=${debian_cloud_image}

	mkdir -p ${outdir}
	wget -O - ${url} | tar xz -C ${outdir} --strip-components=1
}

copy_android_config() {
	local src=$(dirname $0)/fai_config
	local dst=${config_space}

	cp -R ${src}/* ${dst}
	cp $(dirname $0)/image.yaml ${resources_dir}

	local ttyd_version=1.7.7
	local url=https://github.com/tsl0922/ttyd/releases/download/${ttyd_version}/ttyd.aarch64
	mkdir -p ${dst}/files/usr/local/bin/ttyd
	wget ${url} -O ${dst}/files/usr/local/bin/ttyd/AVF
	chmod 777 ${dst}/files/usr/local/bin/ttyd/AVF

        pushd $(dirname $0)/forwarder_guest > /dev/null
		RUSTFLAGS="-C linker=aarch64-linux-gnu-gcc" cargo build \
			--target aarch64-unknown-linux-gnu \
			--target-dir ${workdir}/forwarder_guest
		mkdir -p ${dst}/files/usr/local/bin/forwarder_guest
		cp ${workdir}/forwarder_guest/aarch64-unknown-linux-gnu/debug/forwarder_guest ${dst}/files/usr/local/bin/forwarder_guest/AVF
		chmod 777 ${dst}/files/usr/local/bin/forwarder_guest/AVF
	popd > /dev/null
}

run_fai() {
	local out=${built_image}
	make -C ${debian_cloud_image} image_bookworm_nocloud_arm64
	mv ${debian_cloud_image}/image_bookworm_nocloud_arm64.raw ${out}
}

clean_up() {
	rm -rf ${workdir}
}

set -e
trap clean_up EXIT

built_image=image.raw
workdir=$(mktemp -d)
debian_cloud_image=${workdir}/debian_cloud_image
debian_version=bookworm
config_space=${debian_cloud_image}/config_space/${debian_version}
resources_dir=${debian_cloud_image}/src/debian_cloud_images/resources
check_sudo
parse_options $@
install_prerequisites
download_debian_cloud_image
copy_android_config
run_fai
fdisk -l image.raw
