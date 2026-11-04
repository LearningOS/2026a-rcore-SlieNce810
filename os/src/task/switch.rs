//! Rust wrapper around `__switch`.
//!
//! Switching to a different task's context happens here. The actual
//! implementation must not be in Rust and (essentially) has to be in assembly
//! language (Do you know why?), so this module really is just a wrapper around
//! `switch.S`.

use super::TaskContext;
use core::arch::global_asm;

global_asm!(include_str!("switch.S"));

extern "C" {
    /// Switch to the context of `next_task_cx_ptr`, saving the current context
    /// in `current_task_cx_ptr`.
    pub fn __switch(current_task_cx_ptr: *mut TaskContext, next_task_cx_ptr: *const TaskContext);
}

// 由 switch.S 中的 __switch 直接读写
#[no_mangle]
static mut SWITCH_COUNT: usize = 0;
#[no_mangle]
static mut SWITCH_TICKS: usize = 0;

/// Return '(switch count, accumulated ticks spent inside __switch)'
pub fn _switch_stats() -> (usize, usize) {
    unsafe {
        (SWITCH_TICKS, SWITCH_COUNT)
    }
}