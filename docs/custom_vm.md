# Custom VM

## Headless VMs

If your VM is headless (i.e. console in/out is the primary way of interacting
with it), you can spawn it by passing a JSON config file to the
VirtualizationService via the `vm` tool on a rooted AVF-enabled device. If your
device is attached over ADB, you can run:

```shell
cat > vm_config.json <<EOF
{
  "kernel": "/data/local/tmp/kernel",
  "initrd": "/data/local/tmp/ramdisk",
  "params": "rdinit=/bin/init"
}
EOF
adb root
adb push <kernel> /data/local/tmp/kernel
adb push <ramdisk> /data/local/tmp/ramdisk
adb push vm_config.json /data/local/tmp/vm_config.json
adb shell "/apex/com.android.virt/bin/vm run /data/local/tmp/vm_config.json"
```

The `vm` command also has other subcommands for debugging; run
`/apex/com.android.virt/bin/vm help` for details.

### Running Debian
1. Download an ARM64 image from https://cloud.debian.org/images/cloud/ (We tested nocloud image)

2. Resize the image
```shell
truncate -s 20G debian.img
virt-resize --expand /dev/sda1 <download_image_file> debian.img
```

3. Copy the image file
```shell
tar cfS debian.img.tar debian.img
adb push debian.img.tar /data/local/tmp/
adb shell tar xf /data/local/tmp/debian.img.tar -C /data/local/tmp/
adb shell rm /data/local/tmp/debian.img.tar
adb shell chmod a+w /data/local/tmp/debian.img
rm debian.img.tar
```

Note: we tar and untar to keep the image file sparse.

4. Make the VM config file
```shell
cat > vm_config.json <<EOF
{
    "name": "debian",
    "disks": [
        {
            "image": "/data/local/tmp/debian.img",
            "partitions": [],
            "writable": true
        }
    ],
    "protected": false,
    "cpu_topology": "match_host",
    "platform_version": "~1.0",
    "memory_mib": 8096,
    "debuggable": true,
    "console_out": true,
    "connect_console": true,
    "console_input_device": "ttyS0",
    "network": true,
    "input": {
        "touchscreen": true,
        "keyboard": true,
        "mouse": true,
        "trackpad": true,
        "switches": true
    },
    "audio": {
        "speaker": true,
         "microphone": true
    },
    "gpu": {
        "backend": "virglrenderer",
        "context_types": ["virgl2"]
    },
    "display": {
        "refresh_rate": "30"
    }
}
EOF
adb push vm_config.json /data/local/tmp/
```

5. Launch VmLauncherApp(the detail will be explain below)

6. For console, we can refer to `Debugging` section below. (id: root)

7. For graphical shell, you need to install xfce(for now, only xfce is tested)
```
apt install task-xfce-desktop
dpkg --configure -a (if necessary)
systemctl set-default graphical.target

# need non-root user for graphical shell
adduser linux
# optional
adduser linux sudo
reboot
```
