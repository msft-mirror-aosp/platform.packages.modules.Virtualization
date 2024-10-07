#!/bin/bash

set -e

check_sudo() {
    if [ "$EUID" -ne 0 ]; then
        echo "Please run as root."
        exit
    fi
}

parse_options() {
    if [ -n "$1" ]; then
        out_dir=$1
    else
        out_dir=${PWD}
    fi
}

install_prerequisites() {
    apt update
    apt install --no-install-recommends --assume-yes \
        bpftool \
        clang \
        g++ \
        libbpf-dev \
        libgoogle-glog-dev
}

build_port_listener() {
    cp $(dirname $0)/src/* ${workdir}
    pushd ${workdir} > /dev/null
        bpftool btf dump file /sys/kernel/btf/vmlinux format c > vmlinux.h
        clang \
            -O2 \
            -Wall \
            -target bpf \
            -g \
            -c listen_tracker.ebpf.c \
            -o listen_tracker.ebpf.o
        bpftool gen skeleton listen_tracker.ebpf.o > listen_tracker.skel.h
        clang++ \
            -O2 \
            -Wall \
            -lbpf \
            -lglog \
            -o port_listener \
            main.cc
        cp port_listener ${out_dir}
    popd > /dev/null
}

clean_up() {
    rm -rf ${workdir}
}
trap clean_up EXIT
workdir=$(mktemp -d)

check_sudo
parse_options $@
install_prerequisites
build_port_listener
