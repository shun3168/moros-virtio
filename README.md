The official Moros repository: https://github.com/vinc/moros

virtio-drivers: https://lib.rs/crates/virtio-drivers

# MOROS with VirtIO integration

This fork integrates VirtIOGpu from the virtio-drivers crate (version 0.9.0), making its graphics capabilities usable within Moros. For detailed VirtIOGpu documentation, refer to: https://docs.rs/virtio-drivers/0.9.0/virtio_drivers/device/gpu/struct.VirtIOGpu.html

Additionally the following functions are implemented.
<br/><br/>

**- pub fn get_resolution() -> Option<(u32, u32)>**

Returns the current resolution.
<br/><br/>

**- pub fn draw_square(x: u32, y: u32, color_code: u32)**

Draws 8x8 square at a specified position.

`x`, `y`: Top-left corner coordinates of the square.

`color_code`: A 32-bit color code in 0xAARRGGBB format.
<br/><br/>

**- pub fn draw_image<const W_PIXELS: usize, const H_PIXELS: usize>(image_data_2d: &[[u32; W_PIXELS]; H_PIXELS], dest_x: u32, dest_y: u32,) -> bool**

Displays a image at a specified position. But the image must be converted first to a special .rs format using image/convert_picture.py. You can find a sample data at src/picture_data.rs.

`image_data_2d`: 2D array representing the image, where each inner array is a row of pixels. Each pixel is assumed to be `u32` in 0xAARRGGBB format.

`dest_x`, `dest_y`: Top-left corner coordinates on the screen where the image will be drawn.
<br/><br/>

**- pub fn flush_display() -> bool**

Flush Display to make changes visible.
<br/><br/>

**- pub fn set_pointer(cursor_image: &[u8], cursor_width: u32, cursor_height: u32, hot_x: u32, hot_y: u32, ) -> bool**

Sets the cursor shape and its hotspot.

`cursor_image` should be in RGBA8888 format (4 bytes per pixel).
<br/><br/>

**- pub fn move_pointer(pos_x: u32, pos_y: u32) -> bool**

Moves the cursor to a new position.
<br/><br/>

## Setup

You will need `git`, `gcc`, `make`, `curl`, `qemu-img`,
and `qemu-system-x86_64` on the host system.
You will also need python3 and the Pillow library (pip install Pillow) for image conversion.

Clone the repo:

    $ git clone https://github.com/vinc/moros
    $ cd moros

Install the required tools with `make setup` or the following commands:

    $ curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain none
    $ rustup show
    $ cargo install bootimage

## Usage

Place your desired PNG image file (e.g., my_image.png) into src/image/ and rename it to picture.png.

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

## Modification

Following files are modified from the original.

Cargo.toml, Makefile, src/main.rs, sys/pci.rs, sys/mem/mod.rs, sys/mem/paging.rs, sys/mem/phys.rs

Following files are added.

src/gpu.rs, src/hal.rs, src/picture_data.rs, image/convert_picture.py, image/picture.png, image/picture1.png, image/picture3.png

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
