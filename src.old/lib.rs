#![no_std]
#![cfg_attr(test, no_main)]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(custom_test_frameworks)]
#![feature(ip_from)]
#![feature(naked_functions)]
#![feature(vec_pop_if)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;
extern crate virtio_drivers;

#[macro_use]
pub mod api;

#[macro_use]
pub mod sys;

pub mod usr;

#[macro_use]
pub mod driver;

use bootloader::BootInfo;

const KERNEL_SIZE: usize = 4 << 20; // 4 MB

pub fn init(boot_info: &'static BootInfo) {
    sys::vga::init();
    sys::gdt::init();
    sys::idt::init();
    sys::pic::init(); // Enable interrupts
    sys::serial::init();
    sys::keyboard::init();
    sys::clk::init();

    let v = option_env!("MOROS_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
    log!("SYS MOROS v{}", v);

    sys::mem::init(boot_info);
    sys::cpu::init();
    sys::acpi::init(); // Require MEM
    sys::rng::init();
    sys::pci::init(); // Require MEM    
    sys::net::init(); // Require PCI
    sys::ata::init();
    sys::fs::init(); // Require ATA
        
    // driver::virtio_gpu::init(); // initialize VirtIO
    if let Err(e) = driver::virtio_gpu::init(){
        log!("Failed to initialize VirtIO-GPU: {:?}", e);
    } else {
        log!("VirtIO-GPU initialized.");
    }

    log!("RTC {}", sys::clk::date());
}

#[allow(dead_code)]
#[cfg_attr(not(feature = "userspace"), alloc_error_handler)]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
    let csi_color = api::console::Style::color("red");
    let csi_reset = api::console::Style::reset();
    printk!(
        "{}Error:{} Could not allocate {} bytes\n",
        csi_color,
        csi_reset,
        layout.size()
    );
    hlt_loop();
}

pub trait Testable {
    fn run(&self);
}

impl<T> Testable for T where T: Fn() {
    fn run(&self) {
        print!("test {} ... ", core::any::type_name::<T>());
        self();
        let csi_color = api::console::Style::color("lime");
        let csi_reset = api::console::Style::reset();
        println!("{}ok{}", csi_color, csi_reset);
    }
}

pub fn test_runner(tests: &[&dyn Testable]) {
    let n = tests.len();
    println!("\nrunning {} test{}", n, if n == 1 { "" } else { "s" });
    for test in tests {
        test.run();
    }
    exit_qemu(QemuExitCode::Success);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn exit_qemu(exit_code: QemuExitCode) {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::new(0xF4);
        port.write(exit_code as u32);
    }
}

pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

#[cfg(test)]
use bootloader::entry_point;

#[cfg(test)]
use core::panic::PanicInfo;

#[cfg(test)]
entry_point!(test_kernel_main);

#[cfg(test)]
fn test_kernel_main(boot_info: &'static BootInfo) -> ! {
    init(boot_info);
    test_main();
    hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let csi_color = api::console::Style::color("red");
    let csi_reset = api::console::Style::reset();
    println!("{}failed{}\n", csi_color, csi_reset);
    println!("{}\n", info);
    exit_qemu(QemuExitCode::Failed);
    hlt_loop();
}

#[test_case]
fn trivial_assertion() {
    assert_eq!(1, 1);
}
