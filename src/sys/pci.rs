use alloc::vec;
use alloc::vec::Vec;
use bit_field::BitField;
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::port::Port;
use x86_64::PhysAddr;

#[derive(Debug, Clone, Copy)]
pub struct DeviceConfig {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub status: u16,
    pub command: u16,
    pub rev: u8,
    pub prog: u8,
    pub class: u8,
    pub subclass: u8,
    pub base_addresses: [u32; 6],
    pub interrupt_pin: u8,
    pub interrupt_line: u8,
}

impl DeviceConfig {
    pub fn new(bus: u8, device: u8, function: u8) -> Self {
        let vendor_id = get_vendor_id(bus, device, function);
        let device_id = get_device_id(bus, device, function);

        let mut register = ConfigRegister::new(bus, device, function, 0x04);
        let data = register.read();
        let command = data.get_bits(0..16) as u16;
        let status = data.get_bits(16..32) as u16;

        let mut register = ConfigRegister::new(bus, device, function, 0x08);
        let data = register.read();
        let rev = data.get_bits(0..8) as u8;
        let prog = data.get_bits(8..16) as u8;
        let subclass = data.get_bits(16..24) as u8;
        let class = data.get_bits(24..32) as u8;

        let mut register = ConfigRegister::new(bus, device, function, 0x3C);
        let data = register.read();
        let interrupt_line = data.get_bits(0..8) as u8;
        let interrupt_pin = data.get_bits(8..16) as u8;

        let mut base_addresses: [u32; 6] = [0; 6];
        for i in 0..6 {
            let offset = 0x10 + ((i as u8) << 2);
            let mut register = ConfigRegister::new(
                bus, device, function, offset
            );
            base_addresses[i] = register.read();
        }

        Self {
            bus,
            device,
            function,

            // Configuration Space registers
            vendor_id,
            device_id,
            status,
            command,
            rev,
            prog,
            class,
            subclass,
            base_addresses,
            interrupt_pin,
            interrupt_line,
        }
    }

    pub fn enable_bus_mastering(&mut self) {
        let mut register = ConfigRegister::new(
            self.bus, self.device, self.function, 0x04
        );
        let mut data = register.read();
        data.set_bit(2, true);
        register.write(data);
    }

    pub fn bar_type(&self) -> u16 {
        self.base_addresses[0].get_bits(1..3) as u16
    }

    pub fn mem_base(&self) -> PhysAddr {
        debug_assert!(self.base_addresses[0].get_bit(0) == false);
        let bar0 = self.base_addresses[0];
        // Modified by shshi102
        // let bar1 = self.base_addresses[0];
        let addr = match bar0.get_bits(1..3) {
            0 => { // 32 bits
                (bar0 & 0xFFFFFFF0) as u64
            }
            1 => { // 16 bits
                (bar0 & 0x0000FFF0) as u64
            }
            2 => { // 64 bits
                let l = (bar0 & 0xFFFFFFF0) as u64;

                // Modified to use base_addresses[1], Modified by shshi102
                let h = (self.base_addresses[1] & 0xFFFFFFF0) as u64;
                l + (h << 32)
            }
            _ => { // TODO
                panic!("Unknown base address size");
            }
        };
        PhysAddr::new(addr)
    }

    pub fn io_base(&self) -> u16 {
        debug_assert!(self.base_addresses[0].get_bit(0) == true);
        (self.base_addresses[0] as u16) & 0xFFF0
    }
}

lazy_static! {
    pub static ref PCI_DEVICES: Mutex<Vec<DeviceConfig>> = Mutex::new(vec![]);
}

pub fn find_device(vendor_id: u16, device_id: u16) -> Option<DeviceConfig> {
    for &device in PCI_DEVICES.lock().iter() {
        if device.vendor_id == vendor_id && device.device_id == device_id {
            return Some(device);
        }
    }
    None
}

fn check_bus(bus: u8) {
    for device in 0..32 {
        check_device(bus, device);
    }
}

fn check_device(bus: u8, device: u8) {
    let function = 0;

    let vendor_id = get_vendor_id(bus, device, function);
    if vendor_id == 0xFFFF {
        return; // Device doesn't exist
    }
    add_device(bus, device, function);

    // Multi-function devices
    let header_type = get_header_type(bus, device, function);
    if header_type & 0x80 != 0 {
        for function in 1..8 {
            let vendor_id = get_vendor_id(bus, device, function);
            if vendor_id != 0xFFFF {
                add_device(bus, device, function);
            }
        }
    }
}

fn add_device(bus: u8, device: u8, function: u8) {
    let config = DeviceConfig::new(bus, device, function);
    PCI_DEVICES.lock().push(config);
    log!(
        "PCI {:04X}:{:02X}:{:02X} [{:04X}:{:04X}]",
        bus, device, function, config.vendor_id, config.device_id
    );
}

// Make `read_config` public.
pub fn read_config(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let mut register = ConfigRegister::new(bus, device, function, offset);
    register.read()
}

fn get_vendor_id(bus: u8, device: u8, function: u8) -> u16 {
    read_config(bus, device, function, 0x00).get_bits(0..16) as u16
}

fn get_device_id(bus: u8, device: u8, function: u8) -> u16 {
    read_config(bus, device, function, 0x00).get_bits(16..32) as u16
}

fn get_header_type(bus: u8, device: u8, function: u8) -> u8 {
    read_config(bus, device, function, 0x0C).get_bits(16..24) as u8
}

// Make ConfigRegister public
pub struct ConfigRegister { // ADD `pub`
    data_port: Port<u32>,
    addr_port: Port<u32>,
    addr: u32,
}

impl ConfigRegister {
    // Make new public
    pub fn new(bus: u8, device: u8, function: u8, offset: u8) -> Self { // ADD `pub`
        Self {
            data_port: Port::new(0xCFC),
            addr_port: Port::new(0xCF8),
            addr: 0x8000_0000
                | ((bus as u32) << 16)
                | ((device as u32) << 11)
                | ((function as u32) << 8)
                | ((offset as u32) & 0xFC),
        }
    }

    pub fn read(&mut self) -> u32 {
        unsafe {
            self.addr_port.write(self.addr);
            self.data_port.read()
        }
    }

    // Make write public
    pub fn write(&mut self, data: u32) { // ADD `pub`
        unsafe {
            self.addr_port.write(self.addr);
            self.data_port.write(data);
        }
    }
}

pub fn init() {
    for bus in 0..256 {
        check_bus(bus as u8);
    }

    let devs = PCI_DEVICES.lock();
    for dev in devs.iter() {
        // NOTE: There's not yet an AHCI driver for SATA disks so we must
        // switch to IDE legacy mode for the ATA driver.
        if dev.class == 0x01 && dev.subclass == 0x01 {
            // IDE Controller
            let mut register = ConfigRegister::new(
                dev.bus, dev.device, dev.function, 0x08
            );
            let mut data = register.read();
            let prog_offset = 8; // The programing interface start at bit 8

            // Switching primary channel to compatibility mode
            if dev.prog.get_bit(0) {
                // PCI native mode
                if dev.prog.get_bit(1) {
                    // Modifiable
                    data.set_bit(prog_offset, false);
                    register.write(data);
                }
            }

            // Switching secondary channel to compatibility mode
            if dev.prog.get_bit(2) {
                // PCI native mode
                if dev.prog.get_bit(3) {
                    // Modifiable
                    data.set_bit(prog_offset + 2, false);
                    register.write(data);
                }
            }
        }
    }
}

pub fn list() -> Vec<DeviceConfig> {
    PCI_DEVICES.lock().clone()
}