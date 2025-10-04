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

// Fixed kernel stack for syscalls (temporary solution)
const SYSCALL_STACK_TOP: u64 = 0xFFFF800007E1F000;

// Storage for user RSP during syscall
static mut USER_RSP_SAVE: u64 = 0;

/// Initialize system call support
pub unsafe fn init() {
    SERIAL_PORT.write_str("=== INITIALIZING SYSTEM CALLS ===\n");
    
    // Enable syscall/sysret in EFER
    let mut efer = rdmsr(IA32_EFER);
    efer |= EFER_SCE;
    wrmsr(IA32_EFER, efer);
    SERIAL_PORT.write_str("  Enabled SYSCALL/SYSRET in EFER\n");
    
    // Set STAR: kernel/user code segments
    let star: u64 = 
        ((0x18 | 3) as u64) << 48 |  // User CS (ring 3)
        (0x08 as u64) << 32;          // Kernel CS (ring 0)
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
    let fmask: u64 = 0x200; // Clear IF (bit 9)
    wrmsr(IA32_FMASK, fmask);
    SERIAL_PORT.write_str("  Set FMASK to clear interrupts\n");
    
    SERIAL_PORT.write_str("=== SYSTEM CALL SUPPORT ENABLED ===\n");
}

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

#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    naked_asm!(
        // Save user RSP
        "mov [rip + {user_rsp}], rsp",
        
        // Switch to kernel stack
        "mov rsp, {kernel_stack}",
        "and rsp, 0xFFFFFFFFFFFFFFF0",
        
        // Save registers
        "push r11",
        "push rcx",
        "push r10",
        "push r9",
        "push r8",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rax",
        
        // Rearrange for C calling convention
        "mov r9, r8",
        "mov r8, r10",
        "mov rcx, rdx",
        "mov rdx, rsi",
        "mov rsi, rdi",
        "mov rdi, rax",
        
        // Call handler
        "call {handler}",
        
        // Restore registers
        "add rsp, 8",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop rcx",
        "pop r11",
        
        // Restore user stack
        "mov rsp, [rip + {user_rsp}]",
        
        // Return
        "sysretq",
        
        user_rsp = sym USER_RSP_SAVE,
        kernel_stack = const SYSCALL_STACK_TOP,
        handler = sym syscall_handler_wrapper,
    );
}

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
        result.value
    } else {
        result.value
    }
}

/// Test syscalls by directly calling the handler (NOT using syscall instruction)
pub unsafe fn test_syscall() {
    SERIAL_PORT.write_str("\n=== TESTING SYSTEM CALLS ===\n");
    SERIAL_PORT.write_str("(Direct handler calls - syscall instruction requires user mode)\n\n");
    
    // Test getpid - direct call
    let result = handle_syscall(3, 0, 0, 0, 0, 0);
    SERIAL_PORT.write_str("GetPid returned: ");
    SERIAL_PORT.write_decimal(result.value as u32);
    SERIAL_PORT.write_str("\n");
    
    // Test print - direct call
    let msg = "Hello from syscall!";
    let result = handle_syscall(
        30, 
        msg.as_ptr() as u64, 
        msg.len() as u64,
        0, 0, 0
    );
    SERIAL_PORT.write_str("Print returned: ");
    SERIAL_PORT.write_decimal(result.value as u32);
    SERIAL_PORT.write_str("\n");
    
    // Test gettime - direct call
    let result = handle_syscall(40, 0, 0, 0, 0, 0);
    SERIAL_PORT.write_str("GetTime returned: ");
    SERIAL_PORT.write_decimal(result.value as u32);
    SERIAL_PORT.write_str(" ticks\n");
    
    // Test write - direct call
    let buf = b"Test write\n";
    let result = handle_syscall(
        21,  // Write
        1,   // stdout
        buf.as_ptr() as u64,
        buf.len() as u64,
        0, 0
    );
    SERIAL_PORT.write_str("Write returned: ");
    SERIAL_PORT.write_decimal(result.value as u32);
    SERIAL_PORT.write_str("\n");
    
    SERIAL_PORT.write_str("\n=== SYSCALL TEST COMPLETE ===\n");
    SERIAL_PORT.write_str("Note: syscall instruction will work once you have user-space processes\n\n");
}