use std::ffi::CString;

extern "C" {
    // From miniquad
    fn console_log(msg: *const ::std::os::raw::c_char);
}

pub fn wrap_log(s: &str) {
    let cs = CString::new(s).unwrap();
    unsafe {
        console_log(cs.as_ptr());
    }
}

#[macro_export]
macro_rules! log {
    ($($t:tt)*) => (wrap_log(&format_args!($($t)*).to_string()))
}
