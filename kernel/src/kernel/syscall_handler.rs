// src/kernel/syscall_handler.rs
//! Low-level syscall interrupt setup and handling

use core::arch::asm;
use core::arch::naked_asm;
use crate::kernel::serial::SERIAL_PORT;
use super::syscall::{handle_syscall, SyscallResult};

// MSR registers for syscall
const IA32_STAR: u32 = 0xC0000081;
const IA32_LSTAR: u32 = 0xC0000082;
const IA32_FMASK: u32 = 0xC0000084;
const IA32_EFER: u32 = 0xC0000080;

// EFER bits
const EFER_SCE: u64 = 1 << 0; // System Call Extensions

/// Initialize system call support
pub unsafe fn init() {
    SERIAL_PORT.write_str("=== INITIALIZING SYSTEM CALLS ===\n");
    
    // Enable syscall/sysret in EFER
    let mut efer = rdmsr(IA32_EFER);
    efer |= EFER_SCE;
    wrmsr(IA32_EFER, efer);
    SERIAL_PORT.write_str("  Enabled SYSCALL/SYSRET in EFER\n");
    
    // Set STAR: kernel/user code segments
    // Bits 63:48 = User CS base (will be +16 for user CS, +8 for user SS)
    // Bits 47:32 = Kernel CS base (will be +0 for kernel CS, +8 for kernel SS)
    let star: u64 = 
        ((0x18 | 3) as u64) << 48 |  // User CS (ring 3, GDT entry 3)
        (0x08 as u64) << 32;          // Kernel CS (ring 0, GDT entry 1)
    wrmsr(IA32_STAR, star);
    SERIAL_PORT.write_str("  Set STAR for segment switching\n");
    
    // Set LSTAR: syscall entry point
    let syscall_handler_addr = syscall_entry as *const () as u64;
    wrmsr(IA32_LSTAR, syscall_handler_addr);
    SERIAL_PORT.write_str("  Set LSTAR to syscall handler: 0x");
    SERIAL_PORT.write_hex((syscall_handler_addr >> 32) as u32);
    SERIAL_PORT.write_hex(syscall_handler_addr as u32);
    SERIAL_PORT.write_str("\n");
    
    // Set FMASK: flags to clear on syscall
    // Clear interrupt flag (IF) and others
    let fmask: u64 = 0x200; // Clear IF (bit 9)
    wrmsr(IA32_FMASK, fmask);
    SERIAL_PORT.write_str("  Set FMASK to clear interrupts\n");
    
    SERIAL_PORT.write_str("=== SYSTEM CALL SUPPORT ENABLED ===\n");
}

/// Read Model Specific Register
#[inline]
unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
        options(nomem, nostack, preserves_flags)
    );
    ((high as u64) << 32) | (low as u64)
}

/// Write Model Specific Register
#[inline]
unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
        options(nomem, nostack, preserves_flags)
    );
}

/// System call entry point - called by SYSCALL instruction
/// 
/// Register convention on entry:
/// - RAX: syscall number
/// - RDI: arg1
/// - RSI: arg2
/// - RDX: arg3
/// - R10: arg4  (RCX is clobbered by SYSCALL)
/// - R8:  arg5
/// - R9:  arg6
#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    naked_asm!(
        // Save user stack pointer
        "mov gs:0x10, rsp",      // Save RSP to per-CPU data (or temporary location)
        
        // Switch to kernel stack
        "mov rsp, gs:0x08",      // Load kernel RSP from per-CPU data
        
        // Save all registers
        "push r15",
        "push r14", 
        "push r13",
        "push r12",
        "push r11",  // RFLAGS (saved by SYSCALL)
        "push r10",
        "push r9",
        "push r8",
        "push rbp",
        "push rdi",
        "push rsi",
        "push rdx",
        "push rcx",  // Return RIP (saved by SYSCALL)
        "push rbx",
        "push rax",
        
        // Move syscall arguments to proper positions
        // RAX already has syscall number
        // RDI, RSI, RDX already have arg1, arg2, arg3
        "mov rcx, r10",  // arg4 (R10 was used instead of RCX)
        "mov r9, r8",    // arg5
        // R8 = arg6 (not commonly used)
        
        // Call Rust handler
        "call {handler}",
        
        // Result is now in RAX
        
        // Restore registers (except RAX which has return value)
        "add rsp, 8",    // Skip saved RAX
        "pop rbx",
        "pop rcx",       // Restore return RIP
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop rbp",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",       // Restore RFLAGS
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",
        
        // Restore user stack
        "mov rsp, gs:0x10",
        
        // Return to user space
        "sysretq",
        
        handler = sym syscall_handler_wrapper,
        // options(noreturn)
    );
}

/// Wrapper to call the Rust syscall handler
#[unsafe(no_mangle)]
unsafe extern "C" fn syscall_handler_wrapper(
    syscall_num: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
) -> i64 {
    let result = handle_syscall(syscall_num, arg1, arg2, arg3, arg4, arg5);
    
    if result.error {
        result.value  // Return error code as negative number
    } else {
        result.value  // Return success value
    }
}

/// Test syscall from kernel space (for debugging)
pub unsafe fn test_syscall() {
    SERIAL_PORT.write_str("\n=== TESTING SYSTEM CALL ===\n");
    
    // Test getpid
    let pid: i64;
    asm!(
        "mov rax, 3",    // Syscall::GetPid
        "syscall",
        out("rax") pid,
        out("rcx") _,
        out("r11") _,
    );
    SERIAL_PORT.write_str("GetPid returned: ");
    SERIAL_PORT.write_decimal(pid as u32);
    SERIAL_PORT.write_str("\n");
    
    // Test print
    let msg = "Hello from syscall!";
    let result: i64;
    asm!(
        "mov rax, 30",   // Syscall::Print
        "mov rdi, {msg_ptr}",
        "mov rsi, {msg_len}",
        "syscall",
        msg_ptr = in(reg) msg.as_ptr() as u64,
        msg_len = in(reg) msg.len() as u64,
        out("rax") result,
        out("rcx") _,
        out("r11") _,
        out("rdi") _,
        out("rsi") _,
    );
    SERIAL_PORT.write_str("Print returned: ");
    SERIAL_PORT.write_decimal(result as u32);
    SERIAL_PORT.write_str("\n");
    
    // Test gettime
    let time: i64;
    asm!(
        "mov rax, 40",   // Syscall::GetTime
        "syscall",
        out("rax") time,
        out("rcx") _,
        out("r11") _,
    );
    SERIAL_PORT.write_str("GetTime returned: ");
    SERIAL_PORT.write_decimal(time as u32);
    SERIAL_PORT.write_str(" ticks\n");
    
    SERIAL_PORT.write_str("=== SYSCALL TEST COMPLETE ===\n\n");
}