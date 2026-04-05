// src/kernel/loggers.rs
use super::serial::SERIAL_PORT;
use core::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }

    fn color_code(&self) -> &'static str {
        match self {
            LogLevel::Debug => "\x1b[36m", // Cyan
            LogLevel::Info => "\x1b[32m",   // Green
            LogLevel::Warn => "\x1b[33m",   // Yellow
            LogLevel::Error => "\x1b[31m",  // Red
        }
    }
}

// Use atomic bool for framebuffer availability to avoid mutable static issues
static FRAMEBUFFER_AVAILABLE: AtomicBool = AtomicBool::new(false);

pub struct Logger;

impl Logger {
    pub const fn new() -> Self {
        Self
    }

    pub fn set_framebuffer_available(available: bool) {
        FRAMEBUFFER_AVAILABLE.store(available, Ordering::Relaxed);
    }

    pub fn log(&self, level: LogLevel, message: &str) {
        unsafe {
            // Always log to serial port
            self.log_to_serial(level, message);
            
            // If framebuffer is available, also log there
            if FRAMEBUFFER_AVAILABLE.load(Ordering::Relaxed) {
                self.log_to_framebuffer(level, message);
            }
        }
    }

    unsafe fn log_to_serial(&self, level: LogLevel, message: &str) {
        SERIAL_PORT.write_str("[");
        SERIAL_PORT.write_str(level.as_str());
        SERIAL_PORT.write_str("] ");
        SERIAL_PORT.write_str(message);
        SERIAL_PORT.write_str("\n");
    }

    unsafe fn log_to_framebuffer(&self, _level: LogLevel, _message: &str) {
        // TODO: Implement framebuffer text rendering
        // This would involve drawing characters to the framebuffer
        // For now, this is a placeholder
    }

    // Convenience methods for different log levels
    pub fn debug(&self, message: &str) {
        self.log(LogLevel::Debug, message);
    }

    pub fn info(&self, message: &str) {
        self.log(LogLevel::Info, message);
    }

    pub fn warn(&self, message: &str) {
        self.log(LogLevel::Warn, message);
    }

    pub fn error(&self, message: &str) {
        self.log(LogLevel::Error, message);
    }

    // Hex logging for debugging
    pub fn debug_hex(&self, name: &str, value: u32) {
        unsafe {
            SERIAL_PORT.write_str("[DEBUG] ");
            SERIAL_PORT.write_str(name);
            SERIAL_PORT.write_str(": ");
            SERIAL_PORT.write_hex(value);
            SERIAL_PORT.write_str("\n");
        }
    }

    // Decimal logging for debugging
    pub fn debug_decimal(&self, name: &str, value: u32) {
        unsafe {
            SERIAL_PORT.write_str("[DEBUG] ");
            SERIAL_PORT.write_str(name);
            SERIAL_PORT.write_str(": ");
            SERIAL_PORT.write_decimal(value);
            SERIAL_PORT.write_str("\n");
        }
    }
}

// Global logger instance - now immutable static
pub static LOGGER: Logger = Logger::new();

// Convenience macros
#[macro_export]
macro_rules! log_debug {
    ($($arg:expr),*) => {
        unsafe { $crate::logger::LOGGER.debug(&format_args!($($arg),*)); }
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:expr),*) => {
        unsafe { $crate::logger::LOGGER.info(&format_args!($($arg),*)); }
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:expr),*) => {
        unsafe { $crate::logger::LOGGER.warn(&format_args!($($arg),*)); }
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:expr),*) => {
        unsafe { $crate::logger::LOGGER.error(&format_args!($($arg),*)); }
    };
}