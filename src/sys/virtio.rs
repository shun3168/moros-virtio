use crate::sys::io; // For reading/writing to PCI configuration space
use crate::log;    // For logging

// PCI Configuration Space Registers (Standard Header)
const PCI_CONFIG_ADDRESS: u16 = 0x0CF8;
const PCI_CONFIG_DATA:    u16 = 0x0CFC;

const PCI_VENDOR_ID:    u16 = 0x00; // Offset for Vendor ID in config space
const PCI_DEVICE_ID:    u16 = 0x02; // Offset for Device ID in config space
const PCI_CLASS_DEVICE: u16 = 0x0B; // Offset for Class Code

// VirtIO Vendor and Device IDs (These are examples, double check the VirtIO spec)
const VIRTIO_VENDOR_ID: u16 = 0x1AF4; // VirtIO Vendor ID
const VIRTIO_DEVICE_ID_GPU: u16 = 0x1050; // VirtIO GPU Device ID (This is an example, confirm!)
const VIRTIO_CLASS_CODE: u8 = 0x02; // Class Code for display controller

// Function to read from PCI configuration space
pub fn pci_read_config_u32(bus: u8, device: u8, func: u8, offset: u8) -> u32 {
    let address = 0x80000000 |
                  ((bus as u32) << 16) |
                  ((device as u32) << 11) |
                  ((func as u32) << 8) |
                  (offset as u32);

    unsafe {
        io::outl(PCI_CONFIG_ADDRESS, address);
        io::inl(PCI_CONFIG_DATA)
    }
}

pub fn pci_read_config_u16(bus: u8, device: u8, func: u8, offset: u8) -> u16 {
    pci_read_config_u32(bus, device, func, offset) as u16
}
pub fn pci_read_config_u8(bus: u8, device: u8, func: u8, offset: u8) -> u8 {
    pci_read_config_u32(bus, device, func, offset) as u8
}

// Function to check if a device is a VirtIO-GPU device
pub fn is_virtio_gpu_device(bus: u8, device: u8, func: u8) -> bool {
    let vendor_id = pci_read_config_u16(bus, device, func, PCI_VENDOR_ID);
    let device_id = pci_read_config_u16(bus, device, func, PCI_DEVICE_ID);
    let class_code = pci_read_config_u8(bus, device, func, PCI_CLASS_DEVICE);

    vendor_id == VIRTIO_VENDOR_ID &&
    device_id == VIRTIO_DEVICE_ID_GPU &&
    class_code == VIRTIO_CLASS_CODE
}

// Function to find the VirtIO-GPU device
pub fn find_virtio_gpu() -> Option<(u8, u8, u8)> {
    // Basic PCI enumeration (for demonstration).  A real kernel would do this more robustly.
    for bus in 0..=255 {
        for device in 0..=31 {
            for func in 0..=7 { // Check all functions of a device
                if is_virtio_gpu_device(bus, device, func) {
                    log!("Found VirtIO-GPU device at Bus: {}, Device: {}, Function: {}", bus, device, func);
                    return Some((bus, device, func));
                }
            }
        }
    }
    None // VirtIO-GPU device not found
}
