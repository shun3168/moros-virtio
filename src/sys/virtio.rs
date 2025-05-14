use crate::log;
use x86_64::instructions::port::Port;

// PCI Configuration Space Registers (Standard Header)
const PCI_CONFIG_ADDRESS: u16 = 0x0CF8;
const PCI_CONFIG_DATA: u16 = 0x0CFC;

const PCI_VENDOR_ID_OFFSET: u8 = 0x00;
const PCI_DEVICE_ID_OFFSET: u8 = 0x02;
const PCI_CLASS_DEVICE_OFFSET: u8 = 0x0B;
const PCI_HEADER_TYPE_OFFSET: u8 = 0x0E;

// VirtIO Vendor ID
pub const VIRTIO_VENDOR_ID: u16 = 0x1AF4;

// Common VirtIO Device IDs (You might need to add more as you support other devices)
pub const VIRTIO_DEVICE_ID_GPU: u16 = 0x1050; // Example: VirtIO GPU
pub const VIRTIO_DEVICE_ID_NET: u16 = 0x1000; // Example: VirtIO Network
pub const VIRTIO_DEVICE_ID_BLOCK: u16 = 0x1001; // Example: VirtIO Block

// VirtIO Class Code (Non-VGA)
pub const VIRTIO_CLASS_CODE: u8 = 0x02;

// VirtIO PCI Capability IDs
const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 0x01;
const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 0x02;
const VIRTIO_PCI_CAP_ISR_CFG: u8 = 0x03;
const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 0x04;
const VIRTIO_PCI_CAP_DEVICE_SPECIFIC: u8 = 0x05;
const VIRTIO_PCI_CAP_VENDOR_SPECIFIC: u8 = 0x09;

#[repr(C)]
struct VirtioPciCap {
    cap_vndr: u8,       // 0x00: PCI capability vendor ID (0x09 for vendor-specific)
    cap_next: u8,       // 0x01: Next capability offset
    cap_len: u8,        // 0x02: Capability length
    cfg_type: u8,       // 0x03: VirtIO capability type
    bar: u8,            // 0x04: BAR index
    offset: u32,        // 0x08: Offset within BAR
    length: u32,        // 0x0C: Length of the structure
}

// Function to read from PCI configuration space
pub fn pci_read_config_u32(bus: u8, device: u8, func: u8, offset: u8) -> u32 {
    let address = 0x80000000 |
        ((bus as u32) << 16) |
        ((device as u32) << 11) |
        ((func as u32) << 8) |
        (offset as u32 & 0xFC); // Offset must be 4-byte aligned

    unsafe {
        Port::<u32>::new(PCI_CONFIG_ADDRESS).write(address);
        Port::<u32>::new(PCI_CONFIG_DATA).read()
    }
}

pub fn pci_read_config_u16(bus: u8, device: u8, func: u8, offset: u8) -> u16 {
    pci_read_config_u32(bus, device, func, offset) as u16
}

pub fn pci_read_config_u8(bus: u8, device: u8, func: u8, offset: u8) -> u8 {
    pci_read_config_u32(bus, device, func, offset) as u8
}

// Function to get the header type of a PCI device
pub fn get_pci_header_type(bus: u8, device: u8, func: u8) -> u8 {
    pci_read_config_u8(bus, device, func, PCI_HEADER_TYPE_OFFSET)
}

// Function to check if a device is a VirtIO device
pub fn is_virtio_device(bus: u8, device: u8, func: u8) -> bool {
    let vendor_id = pci_read_config_u16(bus, device, func, PCI_VENDOR_ID_OFFSET);
    vendor_id == VIRTIO_VENDOR_ID
}

// Function to get the VirtIO device ID
pub fn get_virtio_device_id(bus: u8, device: u8, func: u8) -> u16 {
    pci_read_config_u16(bus, device, func, PCI_DEVICE_ID_OFFSET)
}

// Function to find a specific VirtIO capability
pub fn find_virtio_capability(bus: u8, device: u8, func: u8, cap_type: u8) -> Option<*const VirtioPciCap> {
    let header_type = get_pci_header_type(bus, device, func);
    let mut cap_ptr: u8 = if (header_type & 0x0F) == 0x00 {
        // Standard header
        pci_read_config_u8(bus, device, func, 0x34)
    } else if (header_type & 0x0F) == 0x01 {
        // PCI-to-PCI bridge header
        pci_read_config_u8(bus, device, func, 0x40)
    } else {
        return None; // Unknown header type
    };

    if cap_ptr == 0 {
        return None; // No capabilities list
    }

    for _ in 0..48 { // Avoid infinite loops, max 48 capabilities
        let cap_vndr = pci_read_config_u8(bus, device, func, cap_ptr);
        if cap_vndr == 0xFF {
            break; // End of capabilities list
        }
        let cap_next = pci_read_config_u8(bus, device, func, cap_ptr + 1);
        let cfg_type = pci_read_config_u8(bus, device, func, cap_ptr + 3);

        if cap_vndr == 0x09 && cfg_type == cap_type {
            return Some(cap_ptr as *const VirtioPciCap);
        }

        if cap_next == 0 {
            break; // End of capabilities list
        }
        cap_ptr = cap_next;
    }

    None
}

// Function to get the base address of a VirtIO configuration structure
pub fn get_virtio_config_base<T>(bus: u8, device: u8, func: u8, cap_type: u8) -> Option<u64> {
    if let Some(cap_ptr) = find_virtio_capability(bus, device, func, cap_type) {
        let cap = unsafe { &*cap_ptr };
        let bar = cap.bar as usize;
        let offset = cap.offset as u64;

        // Assuming you have a way to get the base address of the BAR
        // This part will depend on how you've implemented BAR handling in pci.rs
        if let Some(base_addr) = crate::sys::pci::get_bar_address(bus, device, func, bar) {
            return Some(base_addr + offset);
        }
    }
    None
}

// --- sys/device/gpu/virtiogpu.rs ---

pub mod virtiogpu {
    use crate::{log, sys::virtio};
    use core::ptr::NonNull;
    use spin::Mutex;

    // VirtIO GPU Constants (from the specification)
    const VIRTIO_GPU_F_VIRGL: u32 = 0;
    const VIRTIO
