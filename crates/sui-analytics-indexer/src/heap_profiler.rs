use std::{ffi::CString, os::raw::c_char, ptr};

extern "C" {
    fn mallctl(
        name: *const c_char,
        oldp: *mut std::ffi::c_void,
        oldlenp: *mut usize,
        newp: *mut std::ffi::c_void,
        newlen: usize,
    ) -> i32;
}

pub fn dump_heap_profile_now() {
    let name = CString::new("prof.dump").unwrap();
    unsafe {
        mallctl(
            name.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        );
    }
}
