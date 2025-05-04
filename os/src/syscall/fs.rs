//! File and filesystem-related syscalls

use alloc::sync::Arc;
use crate::fs::{open_file, find_all_inode_mes, OSInode, OpenFlags, Stat, StatMode, link, unlink};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer};
use crate::task::{current_task, current_user_token};





pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}
// #[cfg(feature = "trait_upcasting")]
/// YOUR JOB: Implement fstat.
pub fn sys_fstat(_fd: usize, _st: *mut Stat) -> isize {
    trace!(
        "kernel:pid[{}] sys_fstat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let p = find_all_inode_mes();
    debug!("fd:{},all:{:?}",_fd,p);
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if let Some(file) = &inner.fd_table[_fd] {
        let file = file.clone();
        drop(inner);
        if let Ok(os_inode) = Arc::downcast::<OSInode>(file) {
            let inner_inode = os_inode.inner.exclusive_access().inode.clone();
            let block_id = inner_inode.block_id;
            let block_offset = inner_inode.block_offset;
            debug!("fd block({},{})",block_id,block_offset);
            let mut matched_id: Option<u32> = None;
            for index in 0..p.len() {
                let (id, (b_id, b_offset)) = p[index];
                if b_id == block_id as u32 && b_offset == block_offset {
                    matched_id = Some(id);
                    break;
                }
            }

            let count = if let Some(id) = matched_id {
                let mut count = 0;
                for index in 0..p.len() {
                    let (item_id, _) = p[index];
                    if item_id == id {
                        count += 1;
                    }
                }
                count
            } else {
                0
            };

            let m;
            if let Some(id) = matched_id {
                if id == 0 {
                    m = StatMode::DIR;
                } else {
                    m = StatMode::FILE;
                }
            } else {
                return -1;
            }

            let stat = Stat {
                dev: 0,
                ino: matched_id.unwrap() as u64,
                mode: m,
                nlink: count,
                pad: [0; 7],
            };

            let stat_bytes = unsafe {
                core::slice::from_raw_parts(
                    &stat as *const Stat as *const u8,
                    core::mem::size_of::<Stat>(),
                )
            };

            let buffers = translated_byte_buffer(current_user_token(), _st as *const u8, core::mem::size_of::<Stat>());
            let mut offset = 0;
            for buffer in buffers {
                let copy_len = core::cmp::min(buffer.len(), stat_bytes.len() - offset);
                buffer[0..copy_len].copy_from_slice(&stat_bytes[offset..offset + copy_len]);
                offset += copy_len;
            }
            return 0;
        }
    }
    -1
}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(_old_name: *const u8, _new_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_linkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let old = translated_str(token, _old_name);
    let new = translated_str(token,_new_name);
    link(old.as_str(),new.as_str())
}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_unlinkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token= current_user_token();
    let name = translated_str(token,_name);
    unlink(name.as_str())
}
