//!Implementation of [`TaskManager`]
use super::TaskControlBlock;
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;
///A array of `TaskControlBlock` that is thread-safe
pub struct TaskManager {
    pub ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

/// A simple FIFO scheduler.
impl TaskManager {
    ///Creat an empty TaskManager
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    /// Add process back to ready queue
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    /// Take a process out of the ready queue
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.ready_queue.pop_front()
    }
}

lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

/// Add process to ready queue
pub fn add_task(task: Arc<TaskControlBlock>) {
    trace!("kernel: TaskManager::add_task,[{:?}]",task.pid);
    TASK_MANAGER.exclusive_access().add(task);
}
// os/src/task/manager.rs
// pub fn add_task(task: Arc<TaskControlBlock>) {
//     let mut manager = TASK_MANAGER.exclusive_access(); // 获取互斥访问权
//     manager.add(task.clone()); // 将任务加入队列（假设 task 是 Arc，克隆引用计数）
//
//     // 新增：打印队列中的所有任务 PID
//     println!("[TASK_MANAGER] Added task, current tasks:");
//     for task_in_queue in manager.ready_queue { // 遍历就绪队列
//         let pid = task_in_queue.getpid(); // 获取任务 PID（假设 TaskControlBlock 有 getpid 方法）
//         println!("- PID: {}", pid);
//     }
//     // 释放互斥访问权（离开作用域时自动释放）
// }



/// Take a process out of the ready queue
pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    //trace!("kernel: TaskManager::fetch_task");
    TASK_MANAGER.exclusive_access().fetch()
}
