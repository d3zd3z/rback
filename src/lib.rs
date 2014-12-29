// The rback library.

#![feature(phase)]

extern crate "rustc-serialize" as rustc_serialize;
extern crate toml;
extern crate libc;
extern crate sudo;

#[phase(plugin,link)]
extern crate log;

pub mod hostname;
pub mod config;
pub mod lvm;

