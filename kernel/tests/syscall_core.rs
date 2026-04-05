#[path = "../src/kernel/syscall_core.rs"]
mod syscall_core;

use std::vec::Vec;

use syscall_core::{
    dispatch, validate_user_range, Syscall, SyscallRequest, SyscallRuntime, SyscallResult,
    SystemInfo, EBADF, EINVAL, ENOSYS,
};

#[derive(Default)]
struct FakeRuntime {
    trace_log: Vec<Syscall>,
    output: Vec<u8>,
    ticks: u64,
    pid: u64,
    sleep_target: Option<u64>,
    system_info: SystemInfo,
}

impl SyscallRuntime for FakeRuntime {
    fn trace(&mut self, syscall: Syscall) {
        self.trace_log.push(syscall);
    }

    fn current_pid(&self) -> u64 {
        self.pid
    }

    fn current_ticks(&self) -> u64 {
        self.ticks
    }

    fn write_console(&mut self, bytes: &[u8]) {
        self.output.extend_from_slice(bytes);
    }

    fn fill_system_info(&self, info: &mut SystemInfo) {
        *info = self.system_info;
    }

    fn sleep_until_tick(&mut self, target_tick: u64) {
        self.sleep_target = Some(target_tick);
    }

    fn exit(&mut self, code: i32) -> ! {
        panic!("unexpected exit({code}) in test runtime");
    }
}

#[test]
fn unknown_syscall_returns_enosys() {
    let mut runtime = FakeRuntime::default();

    let result = unsafe { dispatch(&mut runtime, SyscallRequest::new(0xDEAD, 0, 0, 0, 0, 0)) };

    assert_eq!(result, SyscallResult::err(ENOSYS));
    assert_eq!(runtime.trace_log, vec![Syscall::Invalid]);
}

#[test]
fn write_rejects_bad_file_descriptor() {
    let mut runtime = FakeRuntime::default();
    let payload = *b"hello";

    let result = unsafe {
        dispatch(
            &mut runtime,
            SyscallRequest::new(
                Syscall::Write as u64,
                99,
                payload.as_ptr() as u64,
                payload.len() as u64,
                0,
                0,
            ),
        )
    };

    assert_eq!(result, SyscallResult::err(EBADF));
    assert!(runtime.output.is_empty());
}

#[test]
fn write_stdout_copies_user_buffer() {
    let mut runtime = FakeRuntime::default();
    let payload = *b"hello";

    let result = unsafe {
        dispatch(
            &mut runtime,
            SyscallRequest::new(
                Syscall::Write as u64,
                1,
                payload.as_ptr() as u64,
                payload.len() as u64,
                0,
                0,
            ),
        )
    };

    assert_eq!(result, SyscallResult::ok(payload.len() as i64));
    assert_eq!(runtime.output, payload);
}

#[test]
fn write_rejects_kernel_space_pointer() {
    let mut runtime = FakeRuntime::default();

    let result = unsafe {
        dispatch(
            &mut runtime,
            SyscallRequest::new(
                Syscall::Write as u64,
                1,
                0xFFFF_8000_0000_1000,
                4,
                0,
                0,
            ),
        )
    };

    assert_eq!(result, SyscallResult::err(EINVAL));
}

#[test]
fn getpid_uses_runtime_process_id() {
    let mut runtime = FakeRuntime {
        pid: 42,
        ..FakeRuntime::default()
    };

    let result = unsafe { dispatch(&mut runtime, SyscallRequest::new(Syscall::GetPid as u64, 0, 0, 0, 0, 0)) };

    assert_eq!(result, SyscallResult::ok(42));
}

#[test]
fn sleep_uses_tick_deadline() {
    let mut runtime = FakeRuntime {
        ticks: 25,
        ..FakeRuntime::default()
    };

    let result = unsafe {
        dispatch(
            &mut runtime,
            SyscallRequest::new(Syscall::Sleep as u64, 250, 0, 0, 0, 0),
        )
    };

    assert_eq!(result, SyscallResult::ok(0));
    assert_eq!(runtime.sleep_target, Some(50));
}

#[test]
fn get_system_info_writes_back_to_user_memory() {
    let mut runtime = FakeRuntime {
        system_info: SystemInfo {
            total_memory: 256,
            free_memory: 128,
            uptime_ms: 999,
            process_count: 3,
        },
        ..FakeRuntime::default()
    };
    let mut info = SystemInfo::default();

    let result = unsafe {
        dispatch(
            &mut runtime,
            SyscallRequest::new(
                Syscall::GetSystemInfo as u64,
                &mut info as *mut SystemInfo as u64,
                0,
                0,
                0,
                0,
            ),
        )
    };

    assert_eq!(result, SyscallResult::ok(0));
    assert_eq!(info, runtime.system_info);
}

#[test]
fn validate_user_range_catches_overflow() {
    let err = validate_user_range(u64::MAX - 1, 8).unwrap_err();
    assert_eq!(err, EINVAL);
}
