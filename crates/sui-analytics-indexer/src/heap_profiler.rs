use jemalloc_sys::mallctl;

pub fn dump_heap_profile_now() {
    let name = std::ffi::CString::new("prof.dump").unwrap();
    unsafe {
        mallctl(
            name.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        );
    }
}
