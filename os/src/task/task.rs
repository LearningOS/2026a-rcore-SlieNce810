//! Types related to task management

use super::TaskContext;

/// The task control block (TCB) of a task.
#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    /// The task status in it's lifecycle
    pub task_status: TaskStatus,
    /// The task context
    pub task_cx: TaskContext,
    /// Ticks spent in user mode
    pub user_ticks: usize,
    /// Ticks spent in kernel mode
    pub kernel_ticks: usize,
    /// Tick when the current mode interval started
    pub mode_start: usize,
    /// Tick when the task first got the CPU
    pub first_run: usize,
    /// Whether the task has ever been scheduled
    pub started: bool,
}

/// The status of a task
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Exited,
}
