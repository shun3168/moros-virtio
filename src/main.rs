#![no_std]
#![no_main]

extern crate alloc;

// Modified by shshi102
mod hal; // src/hal.rs Hardware Abstraction Layer for VirtIO Driver
mod gpu; // src/gpu.rs Graphic API Using VirtIO Driver
mod picture_data; // image/picture.rs test image data

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use moros::{
    debug, error, warning, hlt_loop, eprint, eprintln, print, println, sys, usr
};

// Modified by shshi102
use moros::sys::console; // src/sys/console.rs keyboard input
use crate::picture_data::PICTURE_DATA; // image/picture.rs

entry_point!(main);

fn main(boot_info: &'static BootInfo) -> ! {
    moros::init(boot_info);
    print!("\x1b[?25h"); // Ensure cursor

    // Initialize GPU, Modified by shshi102
    gpu::init_and_setup_gpu();
    debug!("Starting VirtIO GPU public API tests...");

    // TEST gpu::get_resolution(), Modified by shshi102
    let (screen_width, screen_height) = match gpu::get_resolution() {
        Some((width, height)) => {
            println!("GPU Resolution: {}x{}", width, height);
            (width, height)
        },
        None => {
            error!("Failed to get GPU resolution. Cannot run tests.");
            hlt_loop();
        }
    };

    // Draw Canvas
    println!("Clearing screen...");
    let square_size: u32 = 8;
    for y_coord in (0..screen_height).step_by(square_size as usize) {
        for x_coord in (0..screen_width).step_by(square_size as usize) {
            gpu::draw_square(x_coord, y_coord, 0xFF000000); // Directly use u32 for black
        }
    }
    gpu::flush_display();

    // TEST gpu::draw_image(), Modified by shshi102
    let picture_data_width = PICTURE_DATA[0].len() as u32;
    let picture_data_height = PICTURE_DATA.len() as u32;

    let picture_start_x = (screen_width / 2).saturating_sub(picture_data_width / 2);
    let picture_start_y = (screen_height / 2).saturating_sub(picture_data_height / 2);
    
    gpu::draw_image(&PICTURE_DATA, picture_start_x, picture_start_y);
    gpu::flush_display();
    println!("Main picture displayed.");

    // Test set_pointer(), Modified by shshi102
    let mut square_x: u32 = (screen_width / 2).saturating_sub(square_size / 2);
    let mut square_y: u32 = (screen_height / 2).saturating_sub(square_size / 2);
    let hotspot_x = gpu::CURSOR_WIDTH / 2;
    let hotspot_y = gpu::CURSOR_HEIGHT / 2;
    if gpu::set_pointer(
        &gpu::CURSOR_DATA,
        gpu::CURSOR_WIDTH,
        gpu::CURSOR_HEIGHT,
        hotspot_x,
        hotspot_y,
    ) {
        println!("Cursor shape and hotspot defined successfully.");
        // Test move_pointer()
        if gpu::move_pointer(square_x + square_size / 2, square_y + square_size / 2) {
            println!("Cursor moved to initial square position,");
        } else {
            error!("Failed to move cursor to initial position.");
        }
    } else {
        error!("Failed to define cursor shape and hotspot.");
    }

    // TEST gpu::draw_square, Modified by shshi102
    println!("Use WASD to draw, 'C' to reset drawing, 'SPACE' to reset drawing and position, 'Q' to quit.");
    let move_step: u32 = 8;
    'keyboard_drawing_loop: loop {
        let input_char = console::read_char();
        match input_char {
            'w' | 'W' => {
                square_y = square_y.saturating_sub(move_step);
                //println!("Move Up at ({}, {})", square_x, square_y);
            },
            's' | 'S' => {
                square_y = square_y.saturating_add(move_step).min(screen_height.saturating_sub(square_size));
                //println!("Move Down at ({}, {})", square_x, square_y);
            },
            'a' | 'A' => {
                square_x = square_x.saturating_sub(move_step);
                //println!("Move Left at ({}, {})", square_x, square_y);
            },
            'd' | 'D' => {
                square_x = square_x.saturating_add(move_step).min(screen_width.saturating_sub(square_size));
                //println!("Move Right at ({}, {})", square_x, square_y);
            },
            'c' | 'C' => { // Clear screen and redraw picture, keep position
                println!("'C' pressed. Resetting screen and redisplaying main picture (keeping position).");
                
                // Clear the entire screen, Modified by shshi102
                for y_coord in (0..screen_height).step_by(square_size as usize) {
                    for x_coord in (0..screen_width).step_by(square_size as usize) {
                        gpu::draw_square(x_coord, y_coord, 0xFF000000); // Directly use u32 for black
                    }
                }
                gpu::flush_display();
                let current_picture_data_width = PICTURE_DATA[0].len() as u32;
                let current_picture_data_height = PICTURE_DATA.len() as u32;
                let current_picture_start_x = (screen_width / 2).saturating_sub(current_picture_data_width / 2);
                let current_picture_start_y = (screen_height / 2).saturating_sub(current_picture_data_height / 2);
                gpu::draw_image(&PICTURE_DATA, current_picture_start_x, current_picture_start_y);
                gpu::flush_display();
            },
            ' ' => {
                println!("'SPACE' pressed. Resetting screen, redisplaying main picture, and centering position.");

                // Clear the entire screen, Modified by shshi102
                for y_coord in (0..screen_height).step_by(square_size as usize) {
                    for x_coord in (0..screen_width).step_by(square_size as usize) {
                        gpu::draw_square(x_coord, y_coord, 0xFF000000); // Directly use u32 for black
                    }
                }
                gpu::flush_display();
                let current_picture_data_width = PICTURE_DATA[0].len() as u32;
                let current_picture_data_height = PICTURE_DATA.len() as u32;
                let current_picture_start_x = (screen_width / 2).saturating_sub(current_picture_data_width / 2);
                let current_picture_start_y = (screen_height / 2).saturating_sub(current_picture_data_height / 2);
                gpu::draw_image(&PICTURE_DATA, current_picture_start_x, current_picture_start_y);
                gpu::flush_display();

                // Reset square position to center, Modified by shshi102
                square_x = (screen_width / 2).saturating_sub(square_size / 2);
                square_y = (screen_height / 2).saturating_sub(square_size / 2);
            },
            'q' | 'Q' => {
                println!("'Q' pressed. Exiting graphics test.");
                println!("Return to command line interface.");
                break 'keyboard_drawing_loop;
            },
            key => {
                debug!("Key pressed: {}", key);
            },
        }

        // Flush drawing and move cursor, Modified by shshi102
        gpu::draw_square(square_x, square_y, 0xFFFF0000);
        gpu::move_pointer(square_x + square_size / 2, square_y + square_size / 2);
        gpu::flush_display();
    }

    loop {
        if let Some(cmd) = option_env!("MOROS_CMD") {
            let prompt = usr::shell::prompt_string(true);
            println!("{}{}", prompt, cmd);
            usr::shell::exec(cmd).ok();
            sys::acpi::shutdown();
        } else {
            user_boot();
        }
    }
}

fn user_boot() {
    let script = "/ini/boot.sh";
    if sys::fs::File::open(script).is_some() {
        usr::shell::main(&["shell", script]).ok();
    } else {
        if sys::fs::is_mounted() {
            error!("Could not find '{}'", script);
        } else {
            warning!("MFS not found, run 'install' to setup the system");
        }
        usr::shell::main(&["shell"]).ok();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    debug!("{}", info);
    hlt_loop();
}
