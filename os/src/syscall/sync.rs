use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use crate::sync::UPSafeCell;
use alloc::sync::Arc;
use alloc::vec::Vec;
use lazy_static::*;

struct DeadlockState {
    /// (pid, tid, sem_id) held by a thread
    held: Vec<(usize, usize, usize)>,
    /// (pid, tid, sem_id) a thread is waiting for
    waiting: Vec<(usize, usize, usize)>,
}

lazy_static! {
    static ref DEADLOCK: UPSafeCell<DeadlockState> = unsafe {
        UPSafeCell::new(DeadlockState {
            held: Vec::new(),
            waiting: Vec::new(),
        })
    };
}

fn deadlocked(
    pid: usize,
    start: usize,
    current: usize,
    held: &[(usize, usize, usize)],
    waiting: &[(usize, usize, usize)],
    visited: &mut Vec<usize>,
) -> bool {
    if current == start {
        return true;
    }
    if visited.contains(&current) {
        return true;
    }
    let sem = match waiting
        .iter()
        .find(|(p, t, _)| *p == pid && *t == current)
    {
        Some(w) => w.2,
        None => return false,
    };
    visited.push(current);
    let holders: Vec<usize> = held
        .iter()
        .filter(|(p, _, s)| *p == pid && *s == sem)
        .map(|(_, t, _)| *t)
        .collect();
    // The thread is blocked only if every holder is blocked as well.
    let result = !holders.is_empty()
        && holders
            .iter()
            .all(|h| deadlocked(pid, start, *h, held, waiting, visited));
    visited.pop();
    result
}

fn has_cycle(pid: usize, tid: usize, sem_id: usize) -> bool {
    let mut state = DEADLOCK.exclusive_access();
    state.waiting.push((pid, tid, sem_id));
    let holders: Vec<usize> = state
        .held
        .iter()
        .filter(|(p, _, s)| *p == pid && *s == sem_id)
        .map(|(_, t, _)| *t)
        .collect();
    let mut visited = Vec::new();
    let result = !holders.is_empty()
        && holders
            .iter()
            .all(|h| deadlocked(pid, tid, *h, &state.held, &state.waiting, &mut visited));
    state.waiting.pop();
    result
}

fn record_held(pid: usize, tid: usize, sem_id: usize) {
    DEADLOCK.exclusive_access().held.push((pid, tid, sem_id));
}

fn record_waiting(pid: usize, tid: usize, sem_id: usize) {
    DEADLOCK
        .exclusive_access()
        .waiting
        .push((pid, tid, sem_id));
}

fn remove_waiting(pid: usize, tid: usize, sem_id: usize) {
    let mut state = DEADLOCK.exclusive_access();
    if let Some(pos) = state
        .waiting
        .iter()
        .position(|&(p, t, s)| p == pid && t == tid && s == sem_id)
    {
        state.waiting.remove(pos);
    }
}

fn remove_held(pid: usize, tid: usize, sem_id: usize) {
    let mut state = DEADLOCK.exclusive_access();
    if let Some(pos) = state
        .held
        .iter()
        .position(|&(p, t, s)| p == pid && t == tid && s == sem_id)
    {
        state.held.remove(pos);
    }
}

/// sleep syscall
pub fn sys_sleep(ms: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_sleep",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}
/// mutex create syscall
pub fn sys_mutex_create(blocking: bool) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.mutex_list.len() as isize - 1
    }
}
/// mutex lock syscall
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_lock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    if mutex.lock_or_deadlock() {
        0
    } else {
        -0xdead
    }
}
/// mutex unlock syscall
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_unlock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.unlock();
    0
}
/// semaphore create syscall
pub fn sys_semaphore_create(res_count: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.semaphore_list.len() - 1
    };
    id as isize
}
/// semaphore up syscall
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_up",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let pid = process.getpid();
    let tid = current_task().unwrap().gettid();
    let process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);
    if crate::sync::deadlock_detect_enabled() {
        remove_held(pid, tid, sem_id);
    }
    sem.up();
    0
}
/// semaphore down syscall
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_down",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let pid = process.getpid();
    let tid = current_task().unwrap().gettid();
    let process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);
    let detect = crate::sync::deadlock_detect_enabled();
    let blocking = sem.inner.exclusive_access().count <= 0;
    if detect && blocking && sem_id != 0 {
        if has_cycle(pid, tid, sem_id) {
            return -0xdead;
        }
        record_waiting(pid, tid, sem_id);
    }
    sem.down();
    if detect && blocking && sem_id != 0 {
        remove_waiting(pid, tid, sem_id);
    }
    if detect {
        record_held(pid, tid, sem_id);
    }
    0
}
/// condvar create syscall
pub fn sys_condvar_create() -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}
/// condvar signal syscall
pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_signal",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}
/// condvar wait syscall
pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_wait",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    condvar.wait(mutex);
    0
}
/// enable deadlock detection syscall
pub fn sys_enable_deadlock_detect(enabled: usize) -> isize {
    trace!("kernel: sys_enable_deadlock_detect");
    crate::sync::set_deadlock_detect(enabled != 0);
    0
}
