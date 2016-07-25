// The rback library.

extern crate chrono;
#[macro_use] extern crate error_chain;
extern crate libc;
extern crate regex;
extern crate rsure;
extern crate rustc_serialize;
extern crate toml;

pub mod config;
pub mod hostname;
pub mod zfs;

pub use zfs::{ZFS, ZfsPath};

pub struct RBack {
    pub host: config::Host,
    pub dry_run: bool,
}
