use std::env::args;
use std::os::raw::{c_char, c_int};
use std::ffi::CString;

extern "C" {
    #[link_name = "\u{1}_Z6c_mainiPPKc"]
    pub fn c_main(
        argc: c_int,
        argv: *mut *const c_char,
    ) -> c_int;
}

fn main() {
    let args: Vec<CString> = args().map(|arg| CString::new(arg).expect("Argument cannot be converted to C string")).collect();
    let mut arg_pointers: Vec<*const c_char> = args.iter().map(|arg| arg.as_ptr()).collect();
    unsafe {
        c_main(args.len() as c_int, arg_pointers.as_mut_ptr());
    }
}
