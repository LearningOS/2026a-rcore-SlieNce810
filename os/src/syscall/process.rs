//! Process management syscalls
use crate::config::{PAGE_SIZE, TRAP_CONTEXT_BASE};
use crate::mm::{user_read_u8, user_write_bytes, user_write_u8};
use crate::task::{
    change_program_brk, current_user_token, exit_current_and_run_next, get_syscall_count,
    suspend_current_and_run_next,
};
use crate::timer::get_time_us;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// get time with second and microsecond
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    let time_val = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    let bytes = unsafe {
        core::slice::from_raw_parts(
            &time_val as *const TimeVal as *const u8,
            core::mem::size_of::<TimeVal>(),
        )
    };
    if user_write_bytes(current_user_token(), ts as usize, bytes) {
        0
    } else {
        -1
    }
}

/// trace syscall: read/write user memory or count syscalls
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    let token = current_user_token();
    match trace_request {
        // read one byte from user address `id`
        0 => match user_read_u8(token, id) {
            Some(value) => value as isize,
            None => -1,
        },
        // write the lowest byte of `data` to user address `id`
        1 => {
            if user_write_u8(token, id, data as u8) {
                0
            } else {
                -1
            }
        }
        // count invocations of syscall `id` by the current task
        2 => get_syscall_count(id).map_or(-1, |count| count as isize),
        _ => -1,
    }
}

/// map `[start, start + len)` with user `port` permissions
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    trace!("kernel: sys_mmap");
    let end = match start.checked_add(len) {
        Some(end) => end,
        None => return -1,
    };
    if start % PAGE_SIZE != 0
        || len == 0
        || port == 0
        || port & !0b111 != 0
        || end >= TRAP_CONTEXT_BASE
    {
        return -1;
    }
    crate::task::mmap(start, len, port)
}

/// unmap `[start, start + len)`
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap");
    if start % PAGE_SIZE != 0 || len == 0 || len % PAGE_SIZE != 0 {
        return -1;
    }
    crate::task::munmap(start, len)
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
