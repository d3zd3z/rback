//! Driver for rback.

#[macro_use] extern crate clap;
extern crate rback;

use clap::{App, Arg, SubCommand};
use std::error;
use std::path::Path;
use std::result;

use rback::ZFS;
use rback::config::Host;

use rback::RBack;

pub type Error = Box<error::Error + Send + Sync>;
pub type Result<T> = result::Result<T, Error>;

fn main() {
    let matches = App::new("rback zfs backup management")
        .version(crate_version!())
        .author("David Brown <davidb@davidb.org>")
        .arg(Arg::with_name("config")
             .short("c")
             .long("config")
             .help("set a custom config file")
             .takes_value(true))
        .arg(Arg::with_name("dry-run")
             .short("n")
             .long("dry-run")
             .help("Don't make modifications to the filesystem"))
        .subcommand(SubCommand::with_name("snap")
                    .about("Take a snapshot"))
        .subcommand(SubCommand::with_name("sure")
                    .about("Update sure info"))
        .subcommand(SubCommand::with_name("prune")
                    .about("Prune old snapshots"))
        .get_matches();

    let config = matches.value_of("config").unwrap_or("backup.toml");

    let cfg = Host::load(&Path::new(config)).unwrap();
    let host = cfg.lookup().unwrap();

    let back = RBack {
        host: host.clone(),
        dry_run: matches.is_present("dry-run"),
    };

    match matches.subcommand_name() {
        None => {
            println!("{}", matches.usage());
            return;
        },
        Some("snap") => do_snap(&back).unwrap(),
        Some("sure") => do_sure(&back).unwrap(),
        Some("prune") => do_prune(&back).unwrap(),
        Some(n) => panic!("Unexpected subcommand name: {}", n),
    }

    // println!("cfg: {:?}", host);
    /*
    let sudo = Sudo::new();
    let zfs = ZFS::new(&sudo, &host.base, &host.snap_prefix);
    zfs.take_snapshot().unwrap();
    */
}

fn do_snap(back: &RBack) -> Result<()> {
    let zfs = ZFS::new(back);
    zfs.take_snapshot()
}

fn do_sure(back: &RBack) -> Result<()> {
    let zfs = ZFS::new(back);
    zfs.run_sure()
}

fn do_prune(back: &RBack) -> Result<()> {
    let zfs = ZFS::new(back);
    zfs.prune_snaps()
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
