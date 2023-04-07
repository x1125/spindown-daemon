# spindown-daemon

Spin-down hard disks without relying on the firmware. It works only on Linux, but might be easily ported to *nix.

It does not rely on any third-party tool.

## Installation

Run `make build` to build and strip the binary at target/release/spindown-daemon.
Run `make install` to copy it to /usr/local/bin/ and add execution bit.

## Usage

`spindown-daemon <DEVICE:TIMEOUT>... -i <check-timeout>`

For example, `spindown-daemon sdb:300 sdc:300 -i 60` will check /dev/sdb and /dev/sdc
every 60 seconds and trigger a spin-down if there's no usage for 300 seconds.

Use `-d` to get debug information.

### Suspend system

Use `--suspend` to suspend the whole system after all disks are asleep.
Use `--suspend-timeout` to wait for n seconds between last sleeping disk and system suspend.
Use `--suspend-check-script` to run a shell script and block system suspend on non-zero exit code.

## Technical details

The checks will use sysfs (`/sys/block/$DEVICE/stat`) to get read and write I/Os to determine device access
and ATA passthrough to get the current power state.

Big thanks to:

* [https://github.com/vthriller/hdd-rs/](https://github.com/vthriller/hdd-rs/)
* [https://github.com/smartmontools/smartmontools/](https://github.com/smartmontools/smartmontools/)
* [https://github.com/hreinecke/sg3_utils/](https://github.com/hreinecke/sg3_utils/)