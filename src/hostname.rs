// Getting the current hostname.

use libc;
use std::io;
use std::io::IoResult;

pub fn get() -> IoResult<String> {
    get_with_capacity(128)
}

fn get_with_capacity(len: uint) -> IoResult<String> {
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

#[cfg(test)]
mod test {
    use std::io::{IoError, OtherIoError};

    // This is more of an OS API test, but it makes sure our assumption about the length is valid,
    // and that we will never call strlen on an unbounded buffer.  It is possible this call will
    // segfault if the gethostname syscall ever returns a success but doesn't null-terminate the
    // buffer.
    #[test]
    fn check_gethostname_termination() {
        let name = super::get().unwrap();

        for i in range(0u, name.len() + 1) {
            match super::get_with_capacity(i) {
                // NameTooLong doesn't get its own error, so we have to match the description
                // string, which is somewhat flaky.
                Err(IoError { kind: OtherIoError, detail: Some(ref msg), ..})
                    if msg.as_slice() == "file name too long" => (),
                Err(e) => panic!("At {}, got {} ({:?}, {}, {:?})", i, e, e.kind, e.desc, e.detail),
                Ok(_) => panic!("Failed and got unterminated buffer"),
            }
        }

        match super::get_with_capacity(name.len() + 1) {
            Ok(ref n2) if &name == n2 => (),
            Ok(ref n2) => panic!("Returned different name: first: {}, second: {}", name, n2),
            Err(e) => panic!("Proper length returned error: {}", e),
        }
    }
}
