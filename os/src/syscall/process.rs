//! Process management syscalls
use crate::mm::{MapPermission, PTEFlags, PageTable, VirtAddr};
use crate::task::{
    change_program_brk, current_syscall_count, current_user_token, exit_current_and_run_next,
    map_current, suspend_current_and_run_next, unmap_current,
};
use crate::timer::get_time_us;
use core::mem::size_of;

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

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    let value = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    let bytes = unsafe {
        core::slice::from_raw_parts((&value as *const TimeVal).cast::<u8>(), size_of::<TimeVal>())
    };
    if copy_to_user(ts.cast::<u8>() as usize, bytes) {
        0
    } else {
        -1
    }
}

/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    match trace_request {
        0 => translate_user_byte(id, false)
            .map(|ptr| unsafe { ptr.read_volatile() as isize })
            .unwrap_or(-1),
        1 => translate_user_byte(id, true)
            .map(|ptr| {
                unsafe { ptr.write_volatile(data as u8) };
                0
            })
            .unwrap_or(-1),
        2 => current_syscall_count(id).map_or(-1, |count| count as isize),
        _ => -1,
    }
}

/// Map anonymous pages with prot bits R=1, W=2 and X=4.
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    trace!("kernel: sys_mmap");
    if port == 0 || port & !0x7 != 0 {
        return -1;
    }
    let mut permission = MapPermission::empty();
    if port & 1 != 0 {
        permission |= MapPermission::R;
    }
    if port & 2 != 0 {
        permission |= MapPermission::W;
    }
    if port & 4 != 0 {
        permission |= MapPermission::X;
    }
    if map_current(start, len, permission) { 0 } else { -1 }
}

/// Remove every page in an existing page-aligned range.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap");
    if unmap_current(start, len) { 0 } else { -1 }
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

/// Translate one user byte and enforce the requested access permission.
fn translate_user_byte(va: usize, write: bool) -> Option<*mut u8> {
    let page_table = PageTable::from_token(current_user_token());
    let va = VirtAddr::from(va);
    let pte = page_table.translate(va.floor())?;
    let flags = pte.flags();
    if !pte.is_valid() || !flags.contains(PTEFlags::U) {
        return None;
    }
    if write && !flags.contains(PTEFlags::W) {
        return None;
    }
    if !write && !flags.contains(PTEFlags::R) {
        return None;
    }
    Some(pte.ppn().get_bytes_array()[va.page_offset()..].as_mut_ptr())
}

/// Copy a kernel byte slice into user memory, including across page boundaries.
fn copy_to_user(dst: usize, src: &[u8]) -> bool {
    for offset in 0..src.len() {
        if translate_user_byte(dst.saturating_add(offset), true).is_none() {
            return false;
        }
    }
    for (offset, byte) in src.iter().enumerate() {
        let ptr = translate_user_byte(dst + offset, true).unwrap();
        unsafe { ptr.write_volatile(*byte) };
    }
    true
}
