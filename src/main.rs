extern crate rback;

extern crate sudo;

use rback::hostname;
use rback::config;
use std::os;

fn main() {
    let host = hostname::get().unwrap();

    let cfg = config::Host::get_host(&Path::new("../config.toml"), host.as_slice()).unwrap();
    println!("cfg: {}", cfg);

    let sudo = sudo::Sudo::new();

    let lvm = rback::lvm::LvmInfo::get(&sudo).unwrap();
    println!("lvm: {}", lvm);
}
