// The rback library.

extern crate chrono;
#[macro_use] extern crate error_type;
extern crate libc;
extern crate regex;
extern crate rustc_serialize;
extern crate toml;

pub mod config;
pub mod hostname;
pub mod zfs;

pub use zfs::ZFS;

pub struct RBack {
    pub host: config::Host,
    pub dry_run: bool,
}
