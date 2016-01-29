// Getting the current hostname.

use libc;
use std::io;

pub fn get() -> io::Result<String> {
    get_with_capacity(128)
}

fn get_with_capacity(len: usize) -> io::Result<String> {
    let mut buf: Vec<u8> = Vec::with_capacity(len);

    // As per the glibc docs, gethostname will always nul terminate the string if the function
    // returns 0.  This means it is safe to call strlen on it.
    match unsafe {
        raw::gethostname(buf.as_ptr() as *mut libc::c_char,
            len as libc::size_t)
    } {
        -1 => Err(io::Error::last_os_error()),
        0 => {
            let len = unsafe { libc::strlen(buf.as_ptr() as *const libc::c_char) };
            unsafe { buf.set_len(len as usize); }
            Ok(String::from_utf8_lossy(&buf[..]).into_owned())
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
    use libc;

    // This is more of an OS API test, but it makes sure our assumption about the length is valid,
    // and that we will never call strlen on an unbounded buffer.  It is possible this call will
    // segfault if the gethostname syscall ever returns a success but doesn't null-terminate the
    // buffer.
    #[test]
    fn check_gethostname_termination() {
        let name = super::get().unwrap();

        for i in 0 .. name.len() + 1 {
            match super::get_with_capacity(i) {
                Err(ref e) if e.raw_os_error() == Some(libc::ENAMETOOLONG) => (),
                Err(ref e) => panic!("At {}, got {} ({:?}, {:?})", i, e, e.kind(), e.raw_os_error()),
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
