// Complete mouse.rs - Replace your entire file with this

use crate::kernel::serial::SERIAL_PORT;
#[derive(Copy, Clone)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Get cursor position from interrupt system - FIXED VERSION

pub struct MouseCursor {
    pub x: i64,
    pub y: i64,
    pub visible: bool,
    pub color: u32,
}

pub fn get_mouse_position() -> Option<(i64, i64)> {
    unsafe {
        use crate::kernel::interrupts::MOUSE_CURSOR;
        // Use addr_of! for safe static access
        let cursor_ptr = core::ptr::addr_of!(MOUSE_CURSOR);
        (*cursor_ptr).as_ref().map(|cursor| cursor.get_position())
    }
}

/// Check if mouse button is pressed - FIXED VERSION
pub fn is_mouse_button_pressed(button: MouseButton) -> bool {
    unsafe {
        use crate::kernel::interrupts::MOUSE_CONTROLLER;
        // Use addr_of! for safe static access
        let controller_ptr = core::ptr::addr_of!(MOUSE_CONTROLLER);
        if let Some(ref mouse) = (*controller_ptr).as_ref() {
            match button {
                MouseButton::Left => mouse.is_left_clicked(),
                MouseButton::Right => mouse.is_right_clicked(),
                MouseButton::Middle => mouse.middle_button,
            }
        } else {
            false
        }
    }
}

impl MouseCursor {
    pub fn new() -> Self {
        Self {
            x: 400,
            y: 300,
            visible: true,
            color: 0xFFFFFFFF, // White
        }
    }

    pub fn update(&mut self, dx: i8, dy: i8, screen_width: u64, screen_height: u64) {
        self.x += dx as i64;
        self.y -= dy as i64;

        self.x = self.x.max(0).min(screen_width as i64 - 1);
        self.y = self.y.max(0).min(screen_height as i64 - 1);
    }

    pub fn get_position(&self) -> (i64, i64) {
        (self.x, self.y)
    }
}

pub struct PS2Mouse {
    packet_buffer: [u8; 3],
    packet_index: usize,
    pub left_button: bool,
    pub right_button: bool,
    pub middle_button: bool,
}

impl PS2Mouse {
    pub fn new() -> Self {
        Self {
            packet_buffer: [0; 3],
            packet_index: 0,
            left_button: false,
            right_button: false,
            middle_button: false,
        }
    }

    // Add this new function to clear any leftover data
    unsafe fn clear_buffer(&self) {
        SERIAL_PORT.write_str("  Clearing mouse buffer...\n");

        // Read any pending data to clear the buffer
        for i in 0..10 {
            let status: u8;
            core::arch::asm!("in al, 0x64", out("al") status);

            if (status & 0x01) != 0 && (status & 0x20) != 0 { // Mouse data available
                let data: u8;
                core::arch::asm!("in al, 0x60", out("al") data);
                SERIAL_PORT.write_str("    Cleared: 0x");
                SERIAL_PORT.write_hex(data as u32);
                SERIAL_PORT.write_str("\n");
            } else {
                break; // No more data
            }
        }
        SERIAL_PORT.write_str("  Buffer cleared\n");
    }

    pub unsafe fn init(&mut self) {
        SERIAL_PORT.write_str("Initializing PS/2 mouse...\n");

        // Step 1: Enable mouse port on PS/2 controller
        SERIAL_PORT.write_str("  Enabling mouse port...\n");
        self.wait_controller_ready();
        core::arch::asm!("out 0x64, al", in("al") 0xA8u8);

        // Step 2: Configure controller for mouse interrupts
        // Step 2: Configure controller for mouse interrupts
        SERIAL_PORT.write_str("  Reading controller config...\n");
        self.wait_controller_ready();
        core::arch::asm!("out 0x64, al", in("al") 0x20u8);

        self.wait_data_ready();
        let mut config: u8;
        core::arch::asm!("in al, 0x60", out("al") config);

        SERIAL_PORT.write_str("    Current config: 0x");
        SERIAL_PORT.write_hex(config as u32);
        SERIAL_PORT.write_str("\n");

        // FIXED: Properly set mouse interrupt bits
        config |= 0x02;  // Enable mouse interrupts (bit 1)
        config &= !0x20; // Enable mouse clock (clear bit 5)
        config |= 0x01;  // Enable keyboard interrupts (keep bit 0 set)

        SERIAL_PORT.write_str("    New config: 0x");
        SERIAL_PORT.write_hex(config as u32);
        SERIAL_PORT.write_str("\n");

        self.wait_controller_ready();
        core::arch::asm!("out 0x64, al", in("al") 0x60u8);
        self.wait_controller_ready();
        core::arch::asm!("out 0x60, al", in("al") config);

        // CRITICAL: Verify the configuration was actually set
        SERIAL_PORT.write_str("  Verifying configuration...\n");
        self.wait_controller_ready();
        core::arch::asm!("out 0x64, al", in("al") 0x20u8);
        self.wait_data_ready();
        let verify_config: u8;
        core::arch::asm!("in al, 0x60", out("al") verify_config);
        SERIAL_PORT.write_str("    Verified config: 0x");
        SERIAL_PORT.write_hex(verify_config as u32);
        if (verify_config & 0x02) != 0 {
            SERIAL_PORT.write_str(" (mouse IRQ enabled)");
        } else {
            SERIAL_PORT.write_str(" (ERROR: mouse IRQ not enabled!)");
        }
        SERIAL_PORT.write_str("\n");

        // Step 3: Clear buffer BEFORE sending commands
        self.clear_buffer();

        // Step 4: Initialize mouse device with proper reset handling
        SERIAL_PORT.write_str("  Initializing mouse device...\n");

        // Reset mouse and handle all responses
        self.send_reset_command();

        // Now send other commands
        self.send_mouse_command(0xF6); // Set defaults
        self.send_mouse_command(0xF4); // Enable reporting

        SERIAL_PORT.write_str("PS/2 mouse initialized\n");
    }
    // Wait for controller to be ready for commands
    // Keep your existing wait functions...
    unsafe fn wait_controller_ready(&self) {
        let mut timeout = 10000;
        while timeout > 0 {
            let status: u8;
            core::arch::asm!("in al, 0x64", out("al") status);
            if (status & 0x02) == 0 {
                return;
            }
            timeout -= 1;
        }
    }

    unsafe fn wait_data_ready(&self) {
        let mut timeout = 10000;
        while timeout > 0 {
            let status: u8;
            core::arch::asm!("in al, 0x64", out("al") status);
            if (status & 0x01) != 0 {
                return;
            }
            timeout -= 1;
        }
    }

    unsafe fn send_mouse_command(&self, command: u8) {
        // Send command prefix
        self.wait_controller_ready();
        core::arch::asm!("out 0x64, al", in("al") 0xD4u8);

        // Send actual command
        self.wait_controller_ready();
        core::arch::asm!("out 0x60, al", in("al") command);

        // Wait for acknowledgment
        self.wait_data_ready();
        let response: u8;
        core::arch::asm!("in al, 0x60", out("al") response);

        if response == 0xFA {
            SERIAL_PORT.write_str("    Command 0x");
            SERIAL_PORT.write_hex(command as u32);
            SERIAL_PORT.write_str(" acknowledged\n");
        } else {
            SERIAL_PORT.write_str("    Command 0x");
            SERIAL_PORT.write_hex(command as u32);
            SERIAL_PORT.write_str(" failed, response: 0x");
            SERIAL_PORT.write_hex(response as u32);
            SERIAL_PORT.write_str("\n");
        }
    }
    pub unsafe fn handle_interrupt(&mut self, cursor: &mut MouseCursor, screen_width: u64, screen_height: u64) {
        // Read data from mouse port
        let data: u8;
        core::arch::asm!("in al, 0x60", out("al") data);

        // Validate first byte of packet (should have bit 3 set)
        if self.packet_index == 0 && (data & 0x08) == 0 {
            SERIAL_PORT.write_str("Invalid packet start, discarding: 0x");
            SERIAL_PORT.write_hex(data as u32);
            SERIAL_PORT.write_str("\n");
            return; // Discard invalid packet start
        }

        self.packet_buffer[self.packet_index] = data;
        self.packet_index += 1;

        if self.packet_index >= 3 {
            self.process_packet(cursor, screen_width, screen_height);
            self.packet_index = 0;
        }
    }

    fn process_packet(&mut self, cursor: &mut MouseCursor, screen_width: u64, screen_height: u64) {
        let flags = self.packet_buffer[0];
        let dx = self.packet_buffer[1] as i8;
        let dy = self.packet_buffer[2] as i8;

        // Update button states
        self.left_button = (flags & 0x01) != 0;
        self.right_button = (flags & 0x02) != 0;
        self.middle_button = (flags & 0x04) != 0;

        // Update cursor position
        cursor.update(dx, dy, screen_width, screen_height);

        unsafe {
            if dx != 0 || dy != 0 {
                SERIAL_PORT.write_str("Mouse: dx=");
                SERIAL_PORT.write_decimal(dx as u32);
                SERIAL_PORT.write_str(" dy=");
                SERIAL_PORT.write_decimal(dy as u32);
                SERIAL_PORT.write_str(" pos=(");
                SERIAL_PORT.write_decimal(cursor.x as u32);
                SERIAL_PORT.write_str(",");
                SERIAL_PORT.write_decimal(cursor.y as u32);
                SERIAL_PORT.write_str(") buttons=");
                if self.left_button { SERIAL_PORT.write_str("L"); }
                if self.right_button { SERIAL_PORT.write_str("R"); }
                if self.middle_button { SERIAL_PORT.write_str("M"); }
                SERIAL_PORT.write_str("\n");
            }
        }
    }

    pub fn is_left_clicked(&self) -> bool {
        self.left_button
    }

    pub fn is_right_clicked(&self) -> bool {
        self.right_button
    }

    unsafe fn send_reset_command(&self) {
        SERIAL_PORT.write_str("    Sending reset command...\n");

        // Send reset command
        self.wait_controller_ready();
        core::arch::asm!("out 0x64, al", in("al") 0xD4u8);
        self.wait_controller_ready();
        core::arch::asm!("out 0x60, al", in("al") 0xFFu8);

        // Read ACK
        self.wait_data_ready();
        let ack: u8;
        core::arch::asm!("in al, 0x60", out("al") ack);
        SERIAL_PORT.write_str("    Reset ACK: 0x");
        SERIAL_PORT.write_hex(ack as u32);
        SERIAL_PORT.write_str("\n");

        // Read self-test result
        self.wait_data_ready();
        let self_test: u8;
        core::arch::asm!("in al, 0x60", out("al") self_test);
        SERIAL_PORT.write_str("    Self-test result: 0x");
        SERIAL_PORT.write_hex(self_test as u32);
        if self_test == 0xAA {
            SERIAL_PORT.write_str(" (PASSED)\n");
        } else {
            SERIAL_PORT.write_str(" (FAILED)\n");
        }

        // Read mouse ID
        self.wait_data_ready();
        let mouse_id: u8;
        core::arch::asm!("in al, 0x60", out("al") mouse_id);
        SERIAL_PORT.write_str("    Mouse ID: 0x");
        SERIAL_PORT.write_hex(mouse_id as u32);
        SERIAL_PORT.write_str("\n");
    }

    // Add this new function for testing
    pub unsafe fn test_mouse_data(&self) {
        SERIAL_PORT.write_str("=== TESTING MOUSE DATA POLLING ===\n");

        for i in 0..10 {
            SERIAL_PORT.write_str("Test ");
            SERIAL_PORT.write_decimal(i + 1);
            SERIAL_PORT.write_str(": ");

            // Check if any data is available from mouse
            let status: u8;
            core::arch::asm!("in al, 0x64", out("al") status);

            if (status & 0x01) != 0 { // Data available
                let data: u8;
                core::arch::asm!("in al, 0x60", out("al") data);
                SERIAL_PORT.write_str("Got data: 0x");
                SERIAL_PORT.write_hex(data as u32);

                // Check if it's from mouse (bit 5 of status)
                if (status & 0x20) != 0 {
                    SERIAL_PORT.write_str(" (from mouse)");
                } else {
                    SERIAL_PORT.write_str(" (from keyboard)");
                }
                SERIAL_PORT.write_str("\n");
            } else {
                SERIAL_PORT.write_str("No data available\n");
            }

            // Small delay
            for _ in 0..100000 { core::arch::asm!("nop"); }
        }

        SERIAL_PORT.write_str("=== END TEST ===\n");
    }

    // Add this new method for manual polling test
    pub unsafe fn poll_for_data(&self) -> bool {
        let status: u8;
        core::arch::asm!("in al, 0x64", out("al") status);

        // Check if data is available (bit 0) and if it's from mouse (bit 5)
        if (status & 0x01) != 0 && (status & 0x20) != 0 {
            let data: u8;
            core::arch::asm!("in al, 0x60", out("al") data);
            SERIAL_PORT.write_str("POLL: Found mouse data: 0x");
            SERIAL_PORT.write_hex(data as u32);
            SERIAL_PORT.write_str("\n");
            return true;
        }
        false
    }
}