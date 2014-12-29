// Getting the current hostname.

use libc;
use std::io;
use std::io::IoResult;

pub fn gethostname() -> IoResult<String> {
    // Guess on the max length
    let len = 128u;

    let mut buf: Vec<u8> = Vec::with_capacity(len);

    // As per the glibc docs, gethostname will always nul terminate the string if the function
    // returns 0.  This means it is safe to call strlen on it.
    match unsafe {
        raw::gethostname(buf.as_ptr() as *mut libc::c_char,
            len as libc::size_t)
    } {
        -1 => Err(io::IoError::last_error()),
        0 => {
            let len = unsafe { libc::strlen(buf.as_ptr() as *const libc::c_char) };
            unsafe { buf.set_len(len as uint); }
            Ok(String::from_utf8_lossy(buf.as_slice()).into_owned())
        },
        _ => panic!("Unexpected result from gethostname"),
    }
}

mod raw {
    use libc;

    extern {
        pub fn gethostname(buf: *mut libc::c_char, len: libc::size_t) -> libc::c_int;
    }
}
