Original Moros: https://github.com/vinc/moros

virtio-drivers: https://lib.rs/crates/virtio-drivers

# Moros with VirtIO integration

VirtIOGpu from virtio-drivers(0.9.0) is integrated and the implementations are made usable. See detail in the documentation (https://docs.rs/virtio-drivers/0.9.0/virtio_drivers/device/gpu/struct.VirtIOGpu.html).

Additionally the following functions are implemented.

## Setup

You will need `git`, `gcc`, `make`, `curl`, `qemu-img`,
and `qemu-system-x86_64` on the host system.

Clone the repo:

    $ git clone https://github.com/vinc/moros
    $ cd moros

Install the required tools with `make setup` or the following commands:

    $ curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain none
    $ rustup show
    $ cargo install bootimage

## Usage

Place any png file to src/image with the name "picture.png"

Build the image to `disk.img`:

    $ make image output=video keyboard=qwerty

Run MOROS in QEMU:

    $ make qemu output=video nic=rtl8139

MOROS will open a console in diskless mode after boot if no filesystem is
detected. The following command will setup the filesystem on a hard drive,
allowing you to exit the diskless mode and log in as a normal user:

    > install

**Be careful not to overwrite the hard drive of your OS when using `dd` inside
your OS, and `install` or `disk format` inside MOROS if you don't use an
emulator.**

## Tests

Run the test suite in QEMU:

    $ make test

## License

MOROS is released under MIT.

[0]: https://vinc.cc
[1]: https://github.com/phil-opp/blog_os/tree/post-07
[2]: https://os.phil-opp.com
[3]: https://wiki.osdev.org
[4]: https://github.com/rust-osdev/bootloader
[5]: https://crates.io/crates/x86_64
[6]: https://crates.io/crates/pic8259
[7]: https://crates.io/crates/pc-keyboard
[8]: https://crates.io/crates/uart_16550
[9]: https://crates.io/crates/linked_list_allocator
[10]: https://crates.io/crates/acpi
[11]: https://crates.io/crates/aml
[12]: https://crates.io/crates/rand_hc
[13]: https://crates.io/crates/smoltcp

[s1]: https://img.shields.io/github/actions/workflow/status/vinc/moros/rust.yml
[s2]: https://img.shields.io/crates/v/moros.svg
