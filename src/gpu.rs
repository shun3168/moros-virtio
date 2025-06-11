use spin::Mutex;
use core::sync::atomic::{AtomicBool, Ordering, AtomicU32};
use lazy_static::lazy_static;
use core::mem;
use virtio_drivers::device::gpu::{
    VirtIOGpu,
};
use virtio_drivers::transport::pci::{
    PciTransport,
    bus::{ConfigurationAccess, PciRoot, DeviceFunction},
};
use virtio_drivers::transport::Transport;

use crate::{debug, error, warning, eprint, eprintln};
use crate::sys::pci;
use crate::hal;

lazy_static! {
    static ref GPU_DRIVER: Mutex<Option<VirtIOGpu<hal::MyKernelHal, PciTransport>>> = Mutex::new(None);
    static ref FRAMEBUFFER_ACCESS: Mutex<Option<&'static mut [u8]>> = Mutex::new(None);

    // Buffer for the cursor image
    static ref INTERNAL_CURSOR_DATA_BUFFER: [u8; (CURSOR_WIDTH * CURSOR_HEIGHT * 4) as usize] = {
        const SMALL_CURSOR_SIZE: u32 = 8;
        const WHITE_PIXEL_BGRA: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
        const TRANSPARENT_PIXEL_BGRA: [u8; 4] = [0x00, 0x00, 0x00, 0x00];

        let mut data = [0u8; (CURSOR_WIDTH * CURSOR_HEIGHT * 4) as usize];

        // Initialize the entire buffer to transparent first
        for chunk in data.chunks_exact_mut(4) {
            chunk.copy_from_slice(&TRANSPARENT_PIXEL_BGRA);
        }

        // Center the smaller white cursor within the 64x64 buffer.
        let start_x = (CURSOR_WIDTH / 2).saturating_sub(SMALL_CURSOR_SIZE / 2);
        let start_y = (CURSOR_HEIGHT / 2).saturating_sub(SMALL_CURSOR_SIZE / 2);

        // Only iterate over the area where the white square should be drawn
        for y in start_y..(start_y + SMALL_CURSOR_SIZE) {
            for x in start_x..(start_x + SMALL_CURSOR_SIZE) {
                let offset = ((y * CURSOR_WIDTH) + x) as usize * 4;
                // Ensure offset is within bounds before writing
                if offset + 4 <= data.len() {
                    data[offset..offset + 4].copy_from_slice(&WHITE_PIXEL_BGRA);
                } else {
                    error!("INTERNAL_CURSOR_DATA_BUFFER: Calculated offset for cursor out of bounds during initialization.");
                }
            }
        }
        data
    };
    pub static ref CURSOR_DATA: &'static [u8] = &INTERNAL_CURSOR_DATA_BUFFER[..];
}

// Boolean whether the GPU has been successfully initialized.
static GPU_INITIALIZED: AtomicBool = AtomicBool::new(false);

// Framebuffer dimensions
static FRAMEBUFFER_WIDTH: AtomicU32 = AtomicU32::new(0);
static FRAMEBUFFER_HEIGHT: AtomicU32 = AtomicU32::new(0);

// virtio-drivers/device/gpu.rs, the CURSOR_RECT is 64x64.
pub const CURSOR_WIDTH: u32 = 64;
pub const CURSOR_HEIGHT: u32 = 64;

// PCI configuration access for virtio-drivers
#[derive(Clone, Copy)]
pub struct MorosPciConfigAccess {
    bus: u8,
    device: u8,
    function: u8,
}

impl MorosPciConfigAccess {
    pub fn new(bus: u8, device: u8, function: u8) -> Self {
        Self { bus, device, function }
    }
}

impl ConfigurationAccess for MorosPciConfigAccess {
    // Reads a 32-bit word from the specified PCI configuration offset.
    fn read_word(&self, _device_function: DeviceFunction, offset: u8) -> u32 {
        pci::read_config(self.bus, self.device, self.function, offset)
    }

    // Writes a 32-bit word to the specified PCI configuration offset.
    fn write_word(&mut self, _device_function: DeviceFunction, offset: u8, val: u32) {
        let mut reg = pci::ConfigRegister::new(self.bus, self.device, self.function, offset);
        reg.write(val);
    }

    unsafe fn unsafe_clone(&self) -> Self {
        *self
    }
}

// Initializes VirtIO GPU driver
// Find PCI device, sets up the transport, GPU driver, Sets up the framebuffer
pub fn init_and_setup_gpu() {
    debug!("Searching for VirtIO GPU...");

    // Find the VirtIO GPU device (Vendor ID: 0x1af4, Device ID: 0x1050).
    let gpu_device_config = pci::find_device(0x1af4, 0x1050);

    if let Some(mut device) = gpu_device_config {
        debug!("Found VirtIO GPU at PCI BDF: {}:{}.{}", device.bus, device.device, device.function);

        // Enable bus mastering for the device for DMA operations.
        device.enable_bus_mastering();
        debug!("Enabled bus mastering for VirtIO GPU.");

        let pci_config_access = MorosPciConfigAccess::new(
            device.bus, device.device, device.function,
        );
        let mut pci_root = PciRoot::new(pci_config_access);
        let device_function = DeviceFunction {
            bus: device.bus, device: device.device, function: device.function,
        };

        // Create a PciTransport for the GPU.
        match PciTransport::new::<hal::MyKernelHal, MorosPciConfigAccess>(
            &mut pci_root,
            device_function
        ) {
            Ok(transport) => {
                debug!("Initialized PciTransport for VirtIO GPU. Type: {:?}", transport.device_type());

                // Check if the discovered device is GPU.
                if transport.device_type() == virtio_drivers::transport::DeviceType::GPU {
                    //debug!("It is VirtIO GPU device");
                    match VirtIOGpu::<hal::MyKernelHal, PciTransport>::new(transport) {
                        Ok(temp_gpu_driver) => {
                            debug!("VirtIO GPU Driver Initialized");
                            let mut driver_guard = GPU_DRIVER.lock();
                            *driver_guard = Some(temp_gpu_driver);
                            let gpu_driver_static_ref: &'static mut VirtIOGpu<hal::MyKernelHal, PciTransport>;
                            unsafe {
                                gpu_driver_static_ref = mem::transmute(driver_guard.as_mut().unwrap());
                            }

                            // Get resolution by 'static mutable reference before setup_framebuffer
                            //because setup_framebuffer internally calls get_display_info.
                            let (w, h) = match gpu_driver_static_ref.resolution() {
                                Ok((w, h)) => {
                                    debug!("Initial GPU resolution detected: {}x{}", w, h);
                                    (w, h)
                                },
                                Err(e) => {
                                    error!("Failed to get initial GPU resolution: {:?}", e);
                                    *driver_guard = None; // Clear the driver if resolution fails
                                    return;
                                }
                            };
                            // Store resolution
                            FRAMEBUFFER_WIDTH.store(w, Ordering::SeqCst);
                            FRAMEBUFFER_HEIGHT.store(h, Ordering::SeqCst);

                            // Use the driver's own `setup_framebuffer` method to handle resource creation,
                            // Allocate necessary DMA memory via `Hal::dma_alloc`.
                            let fb_slice_from_driver = match gpu_driver_static_ref.setup_framebuffer() {
                                Ok(slice) => {
                                    debug!("VirtIO GPU framebuffer setup complete via driver's setup_framebuffer.");
                                    slice
                                },
                                Err(e) => {
                                    error!("Failed to setup VirtIO GPU framebuffer: {:?}", e);
                                    *driver_guard = None; // Clear the driver if setup fails
                                    return;
                                }
                            };

                            let mut fb_access_guard = FRAMEBUFFER_ACCESS.lock();
                            *fb_access_guard = Some(fb_slice_from_driver);
                            GPU_INITIALIZED.store(true, Ordering::Release);
                        }
                        Err(e) => error!("Failed to initialize VirtIO GPU driver: {:?}", e),
                    }
                } else {
                    warning!("Found VirtIO PCI device, but it's not a GPU. Type: {:?}", transport.device_type());
                }
            }
            Err(e) => error!("Failed to create PciTransport: {:?}", e),
        }
    } else {
        warning!("No VirtIO GPU found.");
    }
}

// Returns the current resolution if the GPU driver is initialized.
pub fn get_resolution() -> Option<(u32, u32)> {
    if !GPU_INITIALIZED.load(Ordering::Acquire) {
        return None;
    }
    // Read from stored static values by init_and_setup_gpu
    let width = FRAMEBUFFER_WIDTH.load(Ordering::SeqCst);
    let height = FRAMEBUFFER_HEIGHT.load(Ordering::SeqCst);
    if width > 0 && height > 0 {
        Some((width, height))
    } else {
        None
    }
}

// Accesses the globally stored framebuffer slice for modification.
pub fn with_framebuffer_do<F>(f: F) -> bool
where
    F: FnOnce(&mut [u8], u32, u32),
{
    if !GPU_INITIALIZED.load(Ordering::Acquire) {
        error!("GPU driver not initialized. Cannot get framebuffer.");
        return false;
    }
    let mut fb_access_guard = FRAMEBUFFER_ACCESS.lock();
    if let Some(fb) = fb_access_guard.as_mut() {
        let width = FRAMEBUFFER_WIDTH.load(Ordering::SeqCst);
        let height = FRAMEBUFFER_HEIGHT.load(Ordering::SeqCst);
        // Pass the mutable framebuffer slice and dimensions to the closure.
        f(*fb, width, height);
        true
    } else {
        error!("Framebuffer slice not available (was init successful?).");
        false
    }
}

// Flush Display to make changes visible.
pub fn flush_display() -> bool {
    if !GPU_INITIALIZED.load(Ordering::Acquire) {
        error!("GPU driver not initialized. Cannot flush.");
        return false;
    }
    let mut driver_guard = GPU_DRIVER.lock();
    // Ensure if GPU driver is present
    let gpu_driver = match driver_guard.as_mut() {
        Some(driver) => driver,
        None => {
            error!("GPU driver unexpectedly None when attempting to flush.");
            return false;
        }
    };
    match gpu_driver.flush() {
        Ok(_) => true,
        Err(e) => {
            error!("Error flushing display: {:?}", e);
            false
        }
    }
}

// Sets the cursor shape and its hotspot.
// `cursor_image` should be in RGBA8888 format (4 bytes per pixel).
pub fn set_pointer(
    cursor_image: &[u8],
    cursor_width: u32,
    cursor_height: u32,
    hot_x: u32,
    hot_y: u32,
) -> bool {
    if !GPU_INITIALIZED.load(Ordering::Acquire) {
        error!("GPU driver not initialized. Cannot set pointer.");
        return false;
    }
    // Validate cursor dimensions matching image data length
    if cursor_image.len() != (cursor_width * cursor_height * 4) as usize {
        error!("set_pointer: `cursor_image` length ({}) does not match expected size for {}x{} cursor ({} bytes).",
            cursor_image.len(), cursor_width, cursor_height, (cursor_width * cursor_height * 4) as usize);
        return false;
    }
    // Validate hotspot coordinates
    if hot_x >= cursor_width || hot_y >= cursor_height {
        error!("set_pointer: Hotspot ({},{}) is outside cursor dimensions {}x{}.", hot_x, hot_y, cursor_width, cursor_height);
        return false;
    }

    let mut driver_guard = GPU_DRIVER.lock();
    let gpu_driver = match driver_guard.as_mut() {
        Some(driver) => driver,
        None => {
            error!("GPU driver unexpectedly None when attempting to set pointer.");
            return false;
        }
    };
    match gpu_driver.setup_cursor(cursor_image, cursor_width, cursor_height, hot_x, hot_y) {
        Ok(_) => true,
        Err(e) => {
            error!("Error setting pointer: {:?}", e);
            false
        }
    }
}

/// Moves the cursor to a new position.
pub fn move_pointer(pos_x: u32, pos_y: u32) -> bool {
    if !GPU_INITIALIZED.load(Ordering::Acquire) {
        error!("GPU driver not initialized. Cannot move pointer.");
        return false;
    }
    // Validate position against current framebuffer resolution
    let fb_w = FRAMEBUFFER_WIDTH.load(Ordering::SeqCst);
    let fb_h = FRAMEBUFFER_HEIGHT.load(Ordering::SeqCst);
    if pos_x > fb_w || pos_y > fb_h {
        // Cursor out of bound
        error!("move_pointer: Position ({},{}) is outside screen bounds {}x{}.", pos_x, pos_y, fb_w, fb_h);
        return false;
    }

    let mut driver_guard = GPU_DRIVER.lock();
    let gpu_driver = match driver_guard.as_mut() {
        Some(driver) => driver,
        None => {
            error!("GPU driver unexpectedly None when attempting to move pointer.");
            return false;
        }
    };
    match gpu_driver.move_cursor(pos_x, pos_y) {
        Ok(_) => true,
        Err(e) => {
            error!("Error moving pointer: {:?}", e);
            false
        }
    }
}

// Helper function to draw a single pixel onto the framebuffer.
// Convert 32-bit `color_code` in 0xAARRGGBB format to BGRA format.
fn draw_pixel(framebuffer: &mut [u8], fb_w: u32, _fb_h: u32, px: u32, py: u32, color_code: u32) {

    let bytes_per_pixel = 4; // BGRA format for 4 bytes per pixel
    let offset = ((py * fb_w) + px) as usize * bytes_per_pixel;

    if offset + bytes_per_pixel <= framebuffer.len() {
        // Convert 0xAARRGGBB to BGRA [u8; 4]
        let alpha = ((color_code >> 24) & 0xFF) as u8;
        let red = ((color_code >> 16) & 0xFF) as u8;
        let green = ((color_code >> 8) & 0xFF) as u8;
        let blue = (color_code & 0xFF) as u8;
        let pcolor_bgra = [blue, green, red, alpha];

        framebuffer[offset..offset + bytes_per_pixel].copy_from_slice(&pcolor_bgra);
    } else {
        // Calculation issue or framebuffer corruption.
        error!("draw_pixel: Calculated offset {} + {} bytes out of bounds (framebuffer len: {}). Pixel at ({},{})", offset, bytes_per_pixel, framebuffer.len(), px, py);
    }
}

// Draws 8x8 square at a specified position.
// `x`, `y`: Top-left corner coordinates of the square.
// `color_code`: A 32-bit color code in 0xAARRGGBB format.
pub fn draw_square(x: u32, y: u32, color_code: u32) -> bool {
    const SQUARE_SIZE: u32 = 8;

    // Validate input coordinates against current framebuffer resolution
    let fb_w = FRAMEBUFFER_WIDTH.load(Ordering::SeqCst);
    let fb_h = FRAMEBUFFER_HEIGHT.load(Ordering::SeqCst);

    // If the square is entirely off-screen to the right or bottom, don't even bother drawing.
    // This check is for the top-left corner of the square.
    if x >= fb_w || y >= fb_h {
        debug!("draw_square: Square start ({},{}) is entirely outside screen bounds {}x{}. No drawing performed.", x, y, fb_w, fb_h);
        return false;
    }

    with_framebuffer_do(|framebuffer, fb_w_closure, fb_h_closure| {
        for current_y in y..(y.saturating_add(SQUARE_SIZE)) {
            // Check if current_y exceeds framebuffer height
            if current_y >= fb_h_closure { break; }
            for current_x in x..(x.saturating_add(SQUARE_SIZE)) {
                // Check if current_x exceeds framebuffer width
                if current_x >= fb_w_closure { break; }
                draw_pixel(framebuffer, fb_w_closure, fb_h_closure, current_x, current_y, color_code);
            }
        }
    })
}

// Displays a image at a specified position.
// `image_data_2d`: 2D array representing the image, where each inner array is a row of pixels.
//                   Each pixel is assumed to be `u32` in 0xAARRGGBB format.
// `dest_x`, `dest_y`: Top-left corner coordinates on the screen where the image will be drawn.
pub fn draw_image<const W_PIXELS: usize, const H_PIXELS: usize>(
    image_data_2d: &[[u32; W_PIXELS]; H_PIXELS],
    dest_x: u32,
    dest_y: u32,
) -> bool {
    if !GPU_INITIALIZED.load(Ordering::Acquire) {
        error!("GPU driver not initialized. Cannot draw image.");
        return false;
    }

    // Explicitly check for zero dimensions for the image data itself
    if W_PIXELS == 0 || H_PIXELS == 0 {
        error!("draw_image: Input image has zero width or height ({}x{}).", W_PIXELS, H_PIXELS);
        return false;
    }

    // Get Width and Height
    let image_width = W_PIXELS as u32;
    let image_height = H_PIXELS as u32;

    // Validate that the image is not entirely off-screen
    let fb_w = FRAMEBUFFER_WIDTH.load(Ordering::SeqCst);
    let fb_h = FRAMEBUFFER_HEIGHT.load(Ordering::SeqCst);
    if dest_x >= fb_w || dest_y >= fb_h {
        debug!("draw_image: Image at ({},{}) with dimensions {}x{} is entirely outside screen bounds {}x{}. No drawing performed.",
               dest_x, dest_y, image_width, image_height, fb_w, fb_h);
        return false;
    }

    with_framebuffer_do(|framebuffer, fb_w_closure, fb_h_closure| {
        // Clamp drawing coordinates to screen bounds.
        let start_y = dest_y;
        let end_y = (dest_y.saturating_add(image_height)).min(fb_h_closure);

        let start_x = dest_x;
        let end_x = (dest_x.saturating_add(image_width)).min(fb_w_closure);

        for screen_y in start_y..end_y {
            let y_offset_in_image = screen_y.saturating_sub(dest_y); // Calculate relative y within image_data_2d

            for screen_x in start_x..end_x {
                let x_offset_in_image = screen_x.saturating_sub(dest_x); // Calculate relative x within image_data_2d

                // Get Image Data
                let color_code = image_data_2d[y_offset_in_image as usize][x_offset_in_image as usize];
                draw_pixel(framebuffer, fb_w_closure, fb_h_closure, screen_x, screen_y, color_code);
            }
        }
    })
}