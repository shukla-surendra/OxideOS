use core :: arch ::asm;
use super :: interrupts; 
const PIT_CHANNEL0: u16 = 0x40;
const PIT_COMMAND: u16 = 0x43;

/// Initialize PIT to desired frequency
pub unsafe fn init(hz: u32) {
    let divisor = 1193180 / hz;
    
    // Send command byte
    asm!("out dx, al", in("dx") PIT_COMMAND, in("al") 0x36u8);
    
    // Send frequency divisor
    asm!("out dx, al", in("dx") PIT_CHANNEL0, in("al") (divisor & 0xFF) as u8);
    asm!("out dx, al", in("dx") PIT_CHANNEL0, in("al") ((divisor >> 8) & 0xFF) as u8);
}

/// Get current timer tick count
pub fn get_ticks() -> u64 {
    unsafe { interrupts :: TIMER_TICKS }
}

