//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.

mod context;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use crate::config::{ MAX_APP_NUM, MAX_SYSCALL_NUM };
use crate::loader::{get_num_app, init_app_cx, kernel_stack_top};
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use lazy_static::*;
use riscv::register::sstatus;
use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus};

pub use context::TaskContext;

/// Number of kernel tasks created at boot
const KERNEL_TASK_NUM: usize = 2;

/// A kernel task: busy-watis so that timer
extern "C" fn kernel_task_a() -> ! {
    // First entry leaves __switch with SIE=0; enable interrupts.
    unsafe {
        sstatus::set_sie();
    }
    for round in 0..4 {
        let start = crate::timer::get_time_us();
        while crate::timer::get_time_us() - start < 30_000 {
            core::hint::spin_loop();
        }
        println!(
            "[kernel task A] round {} (kernel timer irq = {})",
            round,
            crate::trap::kernel_timer_count()
        );
        suspend_current_and_run_next();
    }
    report_current_task("kernel task A exit");
    exit_current_and_run_next();
    panic!("kernel task A should not return");
}

/// Another kernel task, with a shorter busy interval
extern "C" fn kernel_task_b() -> ! {
    unsafe {
        sstatus::set_sie();
    }
    for round in 0..4 {
        let start = crate::timer::get_time_us();
        while crate::timer::get_time_us() - start < 20_000 {
            core::hint::spin_loop();
        }
        println!(
            "[kernel task B] round {} (kernel timer irq = {})",
            round,
            crate::trap::kernel_timer_count()
        );
        suspend_current_and_run_next();
    }
    report_current_task("kernel task B exit");
    exit_current_and_run_next();
    panic!("kernel task B should not return");
}

const KERNEL_TASK_ENTRIES: [extern "C" fn() -> !; KERNEL_TASK_NUM] =
    [kernel_task_a, kernel_task_b];

/// The task manager, where all the tasks are managed.
///
/// Functions implemented on `TaskManager` deals with all task state transitions
/// and task context switching. For convenience, you can find wrappers around it
/// in the module level.
///
/// Most of `TaskManager` are hidden behind the field `inner`, to defer
/// borrowing checks to runtime. You can see examples on how to use `inner` in
/// existing functions on `TaskManager`.
pub struct TaskManager {
    /// total number of tasks (user apps + kernel tasks)
    num_tasks: usize,
    /// use inner value to get mutable access
    inner: UPSafeCell<TaskManagerInner>,
}

/// Inner of Task Manager
pub struct TaskManagerInner {
    /// task list
    tasks: [TaskControlBlock; MAX_APP_NUM],
    /// id of current `Running` task
    current_task: usize,
    /// Flat table of syscall invocation counts. The count of syscall 'id'
    /// made by task 'tid' is stored at 'tid *MAX_SYSCALL_NUM + id'
    syscall_count: Vec<u32>,
}

lazy_static! {
    /// Global variable: TASK_MANAGER
    pub static ref TASK_MANAGER: TaskManager = {
        let num_app = get_num_app();
        let num_tasks = num_app + KERNEL_TASK_NUM;
        assert!(num_tasks <= MAX_APP_NUM, "too many tasks");
        let mut tasks = [TaskControlBlock {
            task_cx: TaskContext::zero_init(),
            task_status: TaskStatus::UnInit,
            user_ticks: 0,
            kernel_ticks: 0,
            mode_start: 0,
            first_run: 0,
            started: false,
        }; MAX_APP_NUM];
        for (i, task) in tasks.iter_mut().enumerate() {
            if i < num_app {
                // user task: first run enters through __restore
                task.task_cx = TaskContext::goto_restore(init_app_cx(i));
                task.task_status = TaskStatus::Ready;
            } else if i < num_tasks {
                // kernel task: __switch rets directly into its entry
                task.task_cx = TaskContext::goto_kernel(
                    kernel_stack_top(i),
                    KERNEL_TASK_ENTRIES[i - num_app] as usize,
                );
                task.task_status = TaskStatus::Ready;
            }
        }
        // 20 * 512 * 8 = 80 KiB, too large for the 64 KiB boot stack,
        // so the table lives on the heap.
        let mut syscall_count = Vec::new();
        syscall_count.resize(MAX_APP_NUM * MAX_SYSCALL_NUM, 0);
        TaskManager {
            num_tasks,
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    current_task: 0,
                    syscall_count,
                })
            },
        }
    };
}

impl TaskManager {
    /// Run the first task in task list.
    ///
    /// Generally, the first task in task list is an idle task (we call it zero process later).
    /// But in ch3, we load apps statically, so the first task is a real app.
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let task0 = &mut inner.tasks[0];
        task0.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &task0.task_cx as *const TaskContext;
        let now = crate::timer::get_time();
        inner.tasks[0].mode_start = now;
        inner.tasks[0].first_run = now;
        inner.tasks[0].started = true;
        drop(inner);
        println!("[switch] (first) -> task 0");
        let mut _unused = TaskContext::zero_init();
        // before this, we should drop local variables that must be dropped manually
        unsafe {
            __switch(&mut _unused as *mut TaskContext, next_task_cx_ptr);
        }
        panic!("unreachable in run_first_task!");
    }

    /// Change the status of current `Running` task into `Ready`.
    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Ready;
    }

    /// Change the status of current `Running` task into `Exited`.
    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Exited;
    }

    /// Find next task to run and return task id.
    ///
    /// In this case, we only return the first `Ready` task in task list.
    fn find_next_task(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        (current + 1..current + self.num_tasks + 1)
            .map(|id| id % self.num_tasks)
            .find(|id| inner.tasks[*id].task_status == TaskStatus::Ready)
    }

    /// Record one more invocation of 'syscall_id' by the current task.
    fn add_syscall_count(&self, syscall_id: usize) {
        if syscall_id >= MAX_SYSCALL_NUM { return; }

        let mut inner = self.inner.exclusive_access();
        let index = inner.current_task * MAX_SYSCALL_NUM + syscall_id;
        inner.syscall_count[index] += 1;
    }

    /// return how many times the current task has called 'syscall_id'
    fn get_syscall_count(&self, syscall_id: usize) -> Option<usize> {
        if syscall_id >= MAX_SYSCALL_NUM  {
            return None;
        }
        let inner = self.inner.exclusive_access();
        let index = inner.current_task * MAX_SYSCALL_NUM + syscall_id;
        Some(inner.syscall_count[index] as usize)
    }

    /// Account the interval that ends when a trap is taken.
    ///
    /// `from_user` tells whether the interrupted context was user mode or
    /// supervisor mode (nested interrupt).
    fn on_trap_entry(&self, from_user: bool) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        let now = crate::timer::get_time();
        let task = &mut inner.tasks[current];
        if from_user {
            task.user_ticks += now - task.mode_start;
        } else {
            task.kernel_ticks += now - task.mode_start;
        }
        task.mode_start = now;
    }

    /// Account the kernel-mode interval that ends when a trap return.
    fn on_trap_exit(&self) {
        let mut inner = self.inner.exclusive_access();
        let _current = inner.current_task;
        let task_id = inner.current_task;
        let now = crate::timer::get_time();
        let task = &mut inner.tasks[task_id];
        task.kernel_ticks += now - task.mode_start;
        task.mode_start = now;
    }

    /// Print the user/kernel/wall time of the current task
    fn report_current_task(&self, reason: &str) {
        let (task_id, user_ticks, kernel_ticks, wall_ticks) = {
            let mut inner = self.inner.exclusive_access();
            let task_id = inner.current_task;
            let now = crate::timer::get_time();
            let task = &mut inner.tasks[task_id];
            task.kernel_ticks += now - task.mode_start;
            task.mode_start = now;
            (task_id, task.user_ticks, task.kernel_ticks, now - task.first_run)
        };
        let to_us = |ticks: usize| {
            ticks as u128 * 1_000_000 / crate::config::CLOCK_FREQ as u128
        };
        println!(
            "[time] task {} ({}): user = {} ticks ({} us), kernel = {} ticks ({} us), wall = {} us",
            task_id,
            reason,
            user_ticks,
            to_us(user_ticks),
            kernel_ticks,
            to_us(kernel_ticks),
            to_us(wall_ticks)
        );
    }

    /// Switch current `Running` task to the task we have found,
    /// or there is no `Ready` task and we can exit with all applications completed
    fn run_next_task(&self) {
        // Update task state and call __switch atomically.
        unsafe {
            sstatus::clear_sie();
        }
        if let Some(next) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            inner.tasks[next].task_status = TaskStatus::Running;
            inner.current_task = next;
            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
            // 换出任务此刻在 S 态，先结算它的内核态时间；
            // 换入任务从这一刻开始计时。
            let now = crate::timer::get_time();
            inner.tasks[current].kernel_ticks += now - inner.tasks[current].mode_start;
            inner.tasks[current].mode_start = now;
            inner.tasks[next].mode_start = now;
            if !inner.tasks[next].started {
                inner.tasks[next].started = true;
                inner.tasks[next].first_run = now;
            }
            let _reason = if inner.tasks[current].task_status == TaskStatus::Exited {
                "exit"
            } else {
                "yield/timer"
            };
            drop(inner);
            println!("[switch] task {} ({}) -> task {}", current, _reason, next);
            // before this, we should drop local variables that must be dropped manually
            unsafe {
                __switch(current_task_cx_ptr, next_task_cx_ptr);
                // resumed: restore the kernel interrupt state
                sstatus::set_sie();
            }
            // go back to user mode
        } else {
            Self::print_switch_stats();
            panic!("All applications completed!");
        }
    }

    /// Print how many task switches happened and their average cost.
    pub fn print_switch_stats() {
        let (count, ticks) = switch::switch_stats();
        if count == 0 { return; }
        let avg_milli = ticks * 1000 / count;
        let avg_ns = ticks as u128 * 1_000_000_000
            / (count as u128 * crate::config::CLOCK_FREQ as u128);
        println!(
            "[switch] count = {}, total = {} ticks, average = {}.{:03} ticks (~{} ns)",
            count, ticks, avg_milli / 1000, avg_milli % 1000, avg_ns
        );
    }
}

/// Run the first task in task list.
pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

/// Switch current `Running` task to the task we have found,
/// or there is no `Ready` task and we can exit with all applications completed
fn run_next_task() {
    TASK_MANAGER.run_next_task();
}

/// Change the status of current `Running` task into `Ready`.
fn mark_current_suspended() {
    TASK_MANAGER.mark_current_suspended();
}

/// Change the status of current `Running` task into `Exited`.
fn mark_current_exited() {
    TASK_MANAGER.mark_current_exited();
}

/// Suspend the current 'Running' task and run the next task in task list.
pub fn suspend_current_and_run_next() {
    unsafe {
        sstatus::clear_sie();
    }
    mark_current_suspended();
    run_next_task();
    unsafe {
        sstatus::set_sie();
    }
}

/// Record one more invocation of `syscall_id` by the current task.
pub fn add_syscall_count(syscall_id: usize) {
    TASK_MANAGER.add_syscall_count(syscall_id);
}

/// Return how many times the current task has called `syscall_id`.
pub fn get_syscall_count(syscall_id: usize) -> Option<usize> {
    TASK_MANAGER.get_syscall_count(syscall_id)
}

/// Account the interval that ends when a trap is taken.
pub fn on_trap_entry(from_user: bool) {
    TASK_MANAGER.on_trap_entry(from_user);
}

/// Account the kernel-mode interval that ends when a trap returns.
pub fn on_trap_exit() {
    TASK_MANAGER.on_trap_exit();
}

/// Print the time statistics of the current task.
pub fn report_current_task(reason: &str) {
    TASK_MANAGER.report_current_task(reason);
}


/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next() {
    unsafe {
        sstatus::clear_sie();
    }
    mark_current_exited();
    run_next_task();
}
