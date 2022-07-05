extern "C" {
    fn console_log(len_and_ptr: u32);
}

pub fn wrap_log(s: &str) {
    // Mostly figured this hack out from the following:
    // - https://stackoverflow.com/questions/47529643/how-to-return-a-string-or-similar-from-rust-in-webassembly
    // - https://stackoverflow.com/questions/41353389/how-can-i-return-a-javascript-string-from-a-webassembly-function
    // - https://github.com/not-fl3/miniquad/blob/master/js/gl.js
    unsafe {
        let len = s.len() as u32;
        let ptr = s.as_ptr() as u32;
        console_log((len << 24) | ptr);
    }
}

#[macro_export]
macro_rules! log {
    ($($t:tt)*) => (wrap_log(&format_args!($($t)*).to_string()))
}

//pub(crate) use log;
