//! Driver for rback.

extern crate rback;

use std::path::Path;

use rback::{Sudo, ZFS};

fn main() {
    let cfg = rback::config::Host::load(&Path::new("backup.toml")).unwrap();
    let host = cfg.lookup().unwrap();
    println!("cfg: {:?}", host);
    let sudo = Sudo::new();
    let zfs = ZFS::new(&sudo, &host.base, &host.snap_prefix);
    zfs.take_snapshot().unwrap();
}

/*
extern crate sudo;

use rback::hostname;
use rback::config;
use std::os;

fn main() {
    let host = hostname::get().unwrap();

    let cfg = config::Host::get_host(&Path::new("../config.toml"), host.as_slice()).unwrap();
    println!("cfg: {:?}", cfg);

    let sudo = sudo::Sudo::new();

    let lvm = rback::lvm::LvmInfo::get(&sudo).unwrap();
    println!("lvm: {:?}", lvm);

    let args = os::args();
    if args.len() < 2 {
        usage();
        return;
    }

    match (args[1].as_slice(), args.len()) {
        ("snap", 2) => println!("Snap"),
        ("push", 3) => {
            let dest = args[2].as_slice();
            println!("Push {}", dest);
        },
        _ => println!("Unknown args"),
    }
}

fn usage() {
    println!("Usage: rback {{snap | push dest}}");
}
*/
