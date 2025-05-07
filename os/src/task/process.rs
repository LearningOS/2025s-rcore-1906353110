//! Implementation of  [`ProcessControlBlock`]

// use alloc::collections::VecDeque;
use super::id::RecycleAllocator;
use super::manager::insert_into_pid2process;
use super::TaskControlBlock;
use super::{add_task, SignalFlags};
use super::{pid_alloc, PidHandle};
use crate::fs::{File, Stdin, Stdout};
use crate::mm::{translated_refmut, MemorySet, KERNEL_SPACE};
use crate::sync::{Condvar, Mutex, Semaphore, UPSafeCell};
use crate::trap::{trap_handler, TrapContext};
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::{RefMut};
pub const MAX_SIZE: usize = 5;
// 资源分配状态
pub struct DeadlockDetector {
    available: Vec<usize>,       // 可利用资源向量 (m维)
    allocation: Vec<Vec<usize>>, // 分配矩阵 (n×m维，n=线程数，m=资源类型数)
    need: Vec<Vec<usize>>,       // 需求矩阵 (n×m维)
}

impl DeadlockDetector {
    // 初始化检测器（动态创建矩阵）
    pub fn new() -> Self {
        Self {
            available: vec![0; MAX_SIZE],
            allocation: vec![vec![0; MAX_SIZE]; MAX_SIZE],
            need: vec![vec![0; MAX_SIZE]; MAX_SIZE],
        }
    }
}
impl DeadlockDetector {
    // 执行死锁检测算法
    pub fn detect_deadlock(&self) -> bool {
        let n = self.allocation.len();       // 线程数
        let m = self.available.len();       // 资源类型数
        let mut work = self.available.clone(); // 工作向量
        let mut finish = vec![false; n];    // 结束向量
        // 步骤2：寻找可执行的线程
        loop {
            let mut found = false;
            for i in 0..n {
                if !finish[i] && self.is_resource_sufficient(i, &work) {
                    // 步骤3：分配资源并释放
                    for j in 0..m {
                        work[j] += self.allocation[i][j]; // 释放已分配的资源
                    }
                    finish[i] = true;
                    found = true;
                    break; // 重新开始循环，因为可能有新的线程满足条件
                }
            }
            if !found {
                break; // 无满足条件的线程，退出循环
            }
        }
        // 步骤4：判断是否所有线程都完成
        let is_safe = finish.iter().all(|&f| f);
        !is_safe // 返回true表示发生死锁（不安全状态）
    }

    // 辅助函数：判断线程i的需求是否小于等于工作向量
    fn is_resource_sufficient(&self, thread_idx: usize, work: &[usize]) -> bool {
        let m = self.available.len();
        for j in 0..m {
            if self.need[thread_idx][j] > work[j] {
                return false; // 需求超过可用资源
            }
        }
        true
    }

/// fen sanzhong create put take ，create可以多来几次
    pub fn put_resource(&mut self,request: usize) {
            self.available[request] += 1; // 更新可用资源
    }
    // 模拟资源分配（调用前需先调用detect_deadlock）
    pub fn take_resource(&mut self, thread_idx: usize, request: usize) {
            self.need[thread_idx][request] -= 1; // 更新需求矩阵
            self.allocation[thread_idx][request] += 1; // 更新分配矩阵
            self.available[request] -= 1; // 更新可用资源
    }
    // 模拟资源释放
    pub fn release_resource(&mut self, thread_idx: usize, release: usize) {
            self.allocation[thread_idx][release] -= 1; // 更新分配矩阵
            self.available[release] += 1; // 更新可用资源
    }
    ///要处理要全部情况，如果没有死锁，那就返回0表示成功，不做啥操作，如果死锁了，那就把矩阵都改了。前台也直接返回死锁。
    pub fn detect(&mut self, tid:usize,request:usize)->isize{
        self.need[tid][request] += 1; // 更新需求矩阵
        if self.detect_deadlock()
        {
            self.need[tid][request] -= 1; // 更新需求矩阵
            -0xDEAD
        } else {
            0
        }

    }
}

/// Process Control Block
pub struct ProcessControlBlock {
    /// immutable
    pub pid: PidHandle,
    /// mutable
    inner: UPSafeCell<ProcessControlBlockInner>,
}

/// Inner of Process Control Block
pub struct ProcessControlBlockInner {
    /// is zombie?
    pub is_zombie: bool,
    /// memory set(address space)
    pub memory_set: MemorySet,
    /// parent process
    pub parent: Option<Weak<ProcessControlBlock>>,
    /// children process
    pub children: Vec<Arc<ProcessControlBlock>>,
    /// exit code
    pub exit_code: i32,
    /// file descriptor table
    pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>,
    /// signal flags
    pub signals: SignalFlags,
    /// tasks(also known as threads)
    pub tasks: Vec<Option<Arc<TaskControlBlock>>>,
    /// task resource allocator
    pub task_res_allocator: RecycleAllocator,
    /// mutex list
    pub mutex_list: Vec<Option<Arc<dyn Mutex>>>,
    /// semaphore list
    pub semaphore_list: Vec<Option<Arc<Semaphore>>>,
    /// condvar list
    pub condvar_list: Vec<Option<Arc<Condvar>>>,
    /// detect bool
    pub detect_bool:bool,
    /// mutex detector
    pub mutex_deadlock_detector: Arc<UPSafeCell<DeadlockDetector>>,
    /// semaphore
    pub semaphore_deadlock_detector: Arc<UPSafeCell<DeadlockDetector>>,
}

impl ProcessControlBlockInner {
    #[allow(unused)]
    /// get the address of app's page table
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }
    /// allocate a new file descriptor
    pub fn alloc_fd(&mut self) -> usize {
        if let Some(fd) = (0..self.fd_table.len()).find(|fd| self.fd_table[*fd].is_none()) {
            fd
        } else {
            self.fd_table.push(None);
            self.fd_table.len() - 1
        }
    }
    /// allocate a new task id
    pub fn alloc_tid(&mut self) -> usize {
        self.task_res_allocator.alloc()
    }
    /// deallocate a task id
    pub fn dealloc_tid(&mut self, tid: usize) {
        self.task_res_allocator.dealloc(tid)
    }
    /// the count of tasks(threads) in this process
    pub fn thread_count(&self) -> usize {
        self.tasks.len()
    }
    /// get a task with tid in this process
    pub fn get_task(&self, tid: usize) -> Arc<TaskControlBlock> {
        self.tasks[tid].as_ref().unwrap().clone()
    }
}

impl ProcessControlBlock {
    /// inner_exclusive_access
    pub fn inner_exclusive_access(&self) -> RefMut<'_, ProcessControlBlockInner> {
        self.inner.exclusive_access()
    }
    /// new process from elf file
    pub fn new(elf_data: &[u8]) -> Arc<Self> {
        trace!("kernel: ProcessControlBlock::new");
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
        // allocate a pid
        let pid_handle = pid_alloc();
        let process = Arc::new(Self {
            pid: pid_handle,
            inner: unsafe {
                UPSafeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: vec![
                        // 0 -> stdin
                        Some(Arc::new(Stdin)),
                        // 1 -> stdout
                        Some(Arc::new(Stdout)),
                        // 2 -> stderr
                        Some(Arc::new(Stdout)),
                    ],
                    signals: SignalFlags::empty(),
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(),
                    semaphore_list: Vec::new(),
                    condvar_list: Vec::new(),
                    detect_bool: false,
                    mutex_deadlock_detector: Arc::new(UPSafeCell::new(DeadlockDetector::new())),
                    semaphore_deadlock_detector:Arc::new(UPSafeCell::new(DeadlockDetector::new())),
                })
            },
        });
        // create a main thread, we should allocate ustack and trap_cx here
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&process),
            ustack_base,
            true,
        ));
        // prepare trap_cx of main thread
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        let ustack_top = task_inner.res.as_ref().unwrap().ustack_top();
        let kstack_top = task.kstack.get_top();
        drop(task_inner);
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            ustack_top,
            KERNEL_SPACE.exclusive_access().token(),
            kstack_top,
            trap_handler as usize,
        );
        // add main thread to the process
        let mut process_inner = process.inner_exclusive_access();
        process_inner.tasks.push(Some(Arc::clone(&task)));
        drop(process_inner);
        insert_into_pid2process(process.getpid(), Arc::clone(&process));
        // add main thread to scheduler
        add_task(task);
        process
    }

    /// Only support processes with a single thread.
    pub fn exec(self: &Arc<Self>, elf_data: &[u8], args: Vec<String>) {
        trace!("kernel: exec");
        assert_eq!(self.inner_exclusive_access().thread_count(), 1);
        // memory_set with elf program headers/trampoline/trap context/user stack
        trace!("kernel: exec .. MemorySet::from_elf");
        let (memory_set, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
        let new_token = memory_set.token();
        // substitute memory_set
        trace!("kernel: exec .. substitute memory_set");
        self.inner_exclusive_access().memory_set = memory_set;
        // then we alloc user resource for main thread again
        // since memory_set has been changed
        trace!("kernel: exec .. alloc user resource for main thread again");
        let task = self.inner_exclusive_access().get_task(0);
        let mut task_inner = task.inner_exclusive_access();
        task_inner.res.as_mut().unwrap().ustack_base = ustack_base;
        task_inner.res.as_mut().unwrap().alloc_user_res();
        task_inner.trap_cx_ppn = task_inner.res.as_mut().unwrap().trap_cx_ppn();
        // push arguments on user stack
        trace!("kernel: exec .. push arguments on user stack");
        let mut user_sp = task_inner.res.as_mut().unwrap().ustack_top();
        user_sp -= (args.len() + 1) * core::mem::size_of::<usize>();
        let argv_base = user_sp;
        let mut argv: Vec<_> = (0..=args.len())
            .map(|arg| {
                translated_refmut(
                    new_token,
                    (argv_base + arg * core::mem::size_of::<usize>()) as *mut usize,
                )
            })
            .collect();
        *argv[args.len()] = 0;
        for i in 0..args.len() {
            user_sp -= args[i].len() + 1;
            *argv[i] = user_sp;
            let mut p = user_sp;
            for c in args[i].as_bytes() {
                *translated_refmut(new_token, p as *mut u8) = *c;
                p += 1;
            }
            *translated_refmut(new_token, p as *mut u8) = 0;
        }
        // make the user_sp aligned to 8B for k210 platform
        user_sp -= user_sp % core::mem::size_of::<usize>();
        // initialize trap_cx
        trace!("kernel: exec .. initialize trap_cx");
        let mut trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            task.kstack.get_top(),
            trap_handler as usize,
        );
        trap_cx.x[10] = args.len();
        trap_cx.x[11] = argv_base;
        *task_inner.get_trap_cx() = trap_cx;
    }

    /// Only support processes with a single thread.
    pub fn fork(self: &Arc<Self>) -> Arc<Self> {
        trace!("kernel: fork");
        let mut parent = self.inner_exclusive_access();
        assert_eq!(parent.thread_count(), 1);
        // clone parent's memory_set completely including trampoline/ustacks/trap_cxs
        let memory_set = MemorySet::from_existed_user(&parent.memory_set);
        // alloc a pid
        let pid = pid_alloc();
        // copy fd table
        let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
        for fd in parent.fd_table.iter() {
            if let Some(file) = fd {
                new_fd_table.push(Some(file.clone()));
            } else {
                new_fd_table.push(None);
            }
        }
        // create child process pcb
        let child = Arc::new(Self {
            pid,
            inner: unsafe {
                UPSafeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: new_fd_table,
                    signals: SignalFlags::empty(),
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(),
                    semaphore_list: Vec::new(),
                    condvar_list: Vec::new(),
                    detect_bool: false,
                    mutex_deadlock_detector: Arc::new(UPSafeCell::new(DeadlockDetector::new())),
                    semaphore_deadlock_detector:Arc::new(UPSafeCell::new(DeadlockDetector::new())),
                })
            },
        });
        // add child
        parent.children.push(Arc::clone(&child));
        // create main thread of child process
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&child),
            parent
                .get_task(0)
                .inner_exclusive_access()
                .res
                .as_ref()
                .unwrap()
                .ustack_base(),
            // here we do not allocate trap_cx or ustack again
            // but mention that we allocate a new kstack here
            false,
        ));
        // attach task to child process
        let mut child_inner = child.inner_exclusive_access();
        child_inner.tasks.push(Some(Arc::clone(&task)));
        drop(child_inner);
        // modify kstack_top in trap_cx of this thread
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        trap_cx.kernel_sp = task.kstack.get_top();
        drop(task_inner);
        insert_into_pid2process(child.getpid(), Arc::clone(&child));
        // add this thread to scheduler
        add_task(task);
        child
    }
    /// get pid
    pub fn getpid(&self) -> usize {
        self.pid.0
    }
}


// //! Implementation of  [`ProcessControlBlock`]

// use super::id::RecycleAllocator;
// use super::manager::insert_into_pid2process;
// use super::TaskControlBlock;
// use super::{add_task, SignalFlags};
// use super::{pid_alloc, PidHandle};
// use crate::fs::{File, Stdin, Stdout};
// use crate::mm::{translated_refmut, MemorySet, KERNEL_SPACE};
// use crate::sync::{Condvar, Mutex, Semaphore, UPSafeCell};
// use crate::trap::{trap_handler, TrapContext};
// use alloc::string::String;
// use alloc::sync::{Arc, Weak};
// use alloc::vec;
// use alloc::vec::Vec;
// use core::cell::RefMut;

// /// Process Control Block
// pub struct ProcessControlBlock {
//     /// immutable
//     pub pid: PidHandle,
//     /// mutable
//     inner: UPSafeCell<ProcessControlBlockInner>,
// }

// /// Inner of Process Control Block
// pub struct ProcessControlBlockInner {
//     /// is zombie?
//     pub is_zombie: bool,
//     /// memory set(address space)
//     pub memory_set: MemorySet,
//     /// parent process
//     pub parent: Option<Weak<ProcessControlBlock>>,
//     /// children process
//     pub children: Vec<Arc<ProcessControlBlock>>,
//     /// exit code
//     pub exit_code: i32,
//     /// file descriptor table
//     pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>,
//     /// signal flags
//     pub signals: SignalFlags,
//     /// tasks(also known as threads)
//     pub tasks: Vec<Option<Arc<TaskControlBlock>>>,
//     /// task resource allocator
//     pub task_res_allocator: RecycleAllocator,
//     /// mutex list
//     pub mutex_list: Vec<Option<Arc<dyn Mutex>>>,
//     /// semaphore list
//     pub semaphore_list: Vec<Option<Arc<Semaphore>>>,
//     /// condvar list
//     pub condvar_list: Vec<Option<Arc<Condvar>>>,
// }

// impl ProcessControlBlockInner {
//     #[allow(unused)]
//     /// get the address of app's page table
//     pub fn get_user_token(&self) -> usize {
//         self.memory_set.token()
//     }
//     /// allocate a new file descriptor
//     pub fn alloc_fd(&mut self) -> usize {
//         if let Some(fd) = (0..self.fd_table.len()).find(|fd| self.fd_table[*fd].is_none()) {
//             fd
//         } else {
//             self.fd_table.push(None);
//             self.fd_table.len() - 1
//         }
//     }
//     /// allocate a new task id
//     pub fn alloc_tid(&mut self) -> usize {
//         self.task_res_allocator.alloc()
//     }
//     /// deallocate a task id
//     pub fn dealloc_tid(&mut self, tid: usize) {
//         self.task_res_allocator.dealloc(tid)
//     }
//     /// the count of tasks(threads) in this process
//     pub fn thread_count(&self) -> usize {
//         self.tasks.len()
//     }
//     /// get a task with tid in this process
//     pub fn get_task(&self, tid: usize) -> Arc<TaskControlBlock> {
//         self.tasks[tid].as_ref().unwrap().clone()
//     }
// }

// impl ProcessControlBlock {
//     /// inner_exclusive_access
//     pub fn inner_exclusive_access(&self) -> RefMut<'_, ProcessControlBlockInner> {
//         self.inner.exclusive_access()
//     }
//     /// new process from elf file
//     pub fn new(elf_data: &[u8]) -> Arc<Self> {
//         trace!("kernel: ProcessControlBlock::new");
//         // memory_set with elf program headers/trampoline/trap context/user stack
//         let (memory_set, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
//         // allocate a pid
//         let pid_handle = pid_alloc();
//         let process = Arc::new(Self {
//             pid: pid_handle,
//             inner: unsafe {
//                 UPSafeCell::new(ProcessControlBlockInner {
//                     is_zombie: false,
//                     memory_set,
//                     parent: None,
//                     children: Vec::new(),
//                     exit_code: 0,
//                     fd_table: vec![
//                         // 0 -> stdin
//                         Some(Arc::new(Stdin)),
//                         // 1 -> stdout
//                         Some(Arc::new(Stdout)),
//                         // 2 -> stderr
//                         Some(Arc::new(Stdout)),
//                     ],
//                     signals: SignalFlags::empty(),
//                     tasks: Vec::new(),
//                     task_res_allocator: RecycleAllocator::new(),
//                     mutex_list: Vec::new(),
//                     semaphore_list: Vec::new(),
//                     condvar_list: Vec::new(),
//                 })
//             },
//         });
//         // create a main thread, we should allocate ustack and trap_cx here
//         let task = Arc::new(TaskControlBlock::new(
//             Arc::clone(&process),
//             ustack_base,
//             true,
//         ));
//         // prepare trap_cx of main thread
//         let task_inner = task.inner_exclusive_access();
//         let trap_cx = task_inner.get_trap_cx();
//         let ustack_top = task_inner.res.as_ref().unwrap().ustack_top();
//         let kstack_top = task.kstack.get_top();
//         drop(task_inner);
//         *trap_cx = TrapContext::app_init_context(
//             entry_point,
//             ustack_top,
//             KERNEL_SPACE.exclusive_access().token(),
//             kstack_top,
//             trap_handler as usize,
//         );
//         // add main thread to the process
//         let mut process_inner = process.inner_exclusive_access();
//         process_inner.tasks.push(Some(Arc::clone(&task)));
//         drop(process_inner);
//         insert_into_pid2process(process.getpid(), Arc::clone(&process));
//         // add main thread to scheduler
//         add_task(task);
//         process
//     }

//     /// Only support processes with a single thread.
//     pub fn exec(self: &Arc<Self>, elf_data: &[u8], args: Vec<String>) {
//         trace!("kernel: exec");
//         assert_eq!(self.inner_exclusive_access().thread_count(), 1);
//         // memory_set with elf program headers/trampoline/trap context/user stack
//         trace!("kernel: exec .. MemorySet::from_elf");
//         let (memory_set, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
//         let new_token = memory_set.token();
//         // substitute memory_set
//         trace!("kernel: exec .. substitute memory_set");
//         self.inner_exclusive_access().memory_set = memory_set;
//         // then we alloc user resource for main thread again
//         // since memory_set has been changed
//         trace!("kernel: exec .. alloc user resource for main thread again");
//         let task = self.inner_exclusive_access().get_task(0);
//         let mut task_inner = task.inner_exclusive_access();
//         task_inner.res.as_mut().unwrap().ustack_base = ustack_base;
//         task_inner.res.as_mut().unwrap().alloc_user_res();
//         task_inner.trap_cx_ppn = task_inner.res.as_mut().unwrap().trap_cx_ppn();
//         // push arguments on user stack
//         trace!("kernel: exec .. push arguments on user stack");
//         let mut user_sp = task_inner.res.as_mut().unwrap().ustack_top();
//         user_sp -= (args.len() + 1) * core::mem::size_of::<usize>();
//         let argv_base = user_sp;
//         let mut argv: Vec<_> = (0..=args.len())
//             .map(|arg| {
//                 translated_refmut(
//                     new_token,
//                     (argv_base + arg * core::mem::size_of::<usize>()) as *mut usize,
//                 )
//             })
//             .collect();
//         *argv[args.len()] = 0;
//         for i in 0..args.len() {
//             user_sp -= args[i].len() + 1;
//             *argv[i] = user_sp;
//             let mut p = user_sp;
//             for c in args[i].as_bytes() {
//                 *translated_refmut(new_token, p as *mut u8) = *c;
//                 p += 1;
//             }
//             *translated_refmut(new_token, p as *mut u8) = 0;
//         }
//         // make the user_sp aligned to 8B for k210 platform
//         user_sp -= user_sp % core::mem::size_of::<usize>();
//         // initialize trap_cx
//         trace!("kernel: exec .. initialize trap_cx");
//         let mut trap_cx = TrapContext::app_init_context(
//             entry_point,
//             user_sp,
//             KERNEL_SPACE.exclusive_access().token(),
//             task.kstack.get_top(),
//             trap_handler as usize,
//         );
//         trap_cx.x[10] = args.len();
//         trap_cx.x[11] = argv_base;
//         *task_inner.get_trap_cx() = trap_cx;
//     }

//     /// Only support processes with a single thread.
//     pub fn fork(self: &Arc<Self>) -> Arc<Self> {
//         trace!("kernel: fork");
//         let mut parent = self.inner_exclusive_access();
//         assert_eq!(parent.thread_count(), 1);
//         // clone parent's memory_set completely including trampoline/ustacks/trap_cxs
//         let memory_set = MemorySet::from_existed_user(&parent.memory_set);
//         // alloc a pid
//         let pid = pid_alloc();
//         // copy fd table
//         let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
//         for fd in parent.fd_table.iter() {
//             if let Some(file) = fd {
//                 new_fd_table.push(Some(file.clone()));
//             } else {
//                 new_fd_table.push(None);
//             }
//         }
//         // create child process pcb
//         let child = Arc::new(Self {
//             pid,
//             inner: unsafe {
//                 UPSafeCell::new(ProcessControlBlockInner {
//                     is_zombie: false,
//                     memory_set,
//                     parent: Some(Arc::downgrade(self)),
//                     children: Vec::new(),
//                     exit_code: 0,
//                     fd_table: new_fd_table,
//                     signals: SignalFlags::empty(),
//                     tasks: Vec::new(),
//                     task_res_allocator: RecycleAllocator::new(),
//                     mutex_list: Vec::new(),
//                     semaphore_list: Vec::new(),
//                     condvar_list: Vec::new(),
//                 })
//             },
//         });
//         // add child
//         parent.children.push(Arc::clone(&child));
//         // create main thread of child process
//         let task = Arc::new(TaskControlBlock::new(
//             Arc::clone(&child),
//             parent
//                 .get_task(0)
//                 .inner_exclusive_access()
//                 .res
//                 .as_ref()
//                 .unwrap()
//                 .ustack_base(),
//             // here we do not allocate trap_cx or ustack again
//             // but mention that we allocate a new kstack here
//             false,
//         ));
//         // attach task to child process
//         let mut child_inner = child.inner_exclusive_access();
//         child_inner.tasks.push(Some(Arc::clone(&task)));
//         drop(child_inner);
//         // modify kstack_top in trap_cx of this thread
//         let task_inner = task.inner_exclusive_access();
//         let trap_cx = task_inner.get_trap_cx();
//         trap_cx.kernel_sp = task.kstack.get_top();
//         drop(task_inner);
//         insert_into_pid2process(child.getpid(), Arc::clone(&child));
//         // add this thread to scheduler
//         add_task(task);
//         child
//     }
//     /// get pid
//     pub fn getpid(&self) -> usize {
//         self.pid.0
//     }
// }
