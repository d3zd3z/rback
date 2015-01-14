// The rback library.

#![allow(unstable)]
#![feature(int_uint)]

extern crate "rustc-serialize" as rustc_serialize;
extern crate toml;
extern crate libc;
extern crate sudo;

#[macro_use]
extern crate log;

pub mod hostname;
pub mod config;
pub mod lvm;

