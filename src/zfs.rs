// ZFS support

use chrono::{Datelike, Local};
use regex::{self, Regex};
use rsure;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::error;
use std::path::Path;
use std::io::prelude::*;
use std::io::{BufReader};
use std::process::Command;
use std::result;

// For dev, boxed errors.
pub type Error = Box<error::Error + Send + Sync>;
pub type Result<T> = result::Result<T, Error>;

use RBack;

// For pruning, always keep at least this many of the pruned snapshots.
const PRUNE_KEEP: usize = 10;

pub struct ZFS<'a> {
    back: &'a RBack,
    snap_re: Regex,
}

impl<'a> ZFS<'a> {
    pub fn new<'b>(back: &'b RBack) -> ZFS<'b> {
        let quoted = regex::quote(&back.host.snap_prefix);
        let pat = format!("^{}(\\d+)[-\\.]([-\\.\\d]+)$", quoted);
        ZFS {
            back: back,
            snap_re: Regex::new(&pat).unwrap(),
        }
    }

    pub fn get_snaps(&self, dir: &str) -> Result<Vec<DataSet>> {
        let mut cmd = Command::new("zfs");
        cmd.args(&["list", "-H", "-t", "all", "-o", "name,mountpoint",
                 "-r", dir]);
        let out = try!(cmd.output());
        if !out.status.success() {
            return Err(format!("zfs list returned error: {:?}", out.status).into());
        }
        let buf = out.stdout;
        println!("Len: {} bytes", buf.len());

        let mut builder = SnapBuilder::new();

        for line in BufReader::new(&buf[..]).lines() {
            let line = try!(line);
            let fields: Vec<_> = line.splitn(2, '\t').collect();
            if fields.len() != 2 {
                return Err(format!("zfs line doesn't have two fields: {:?}", line).into());
            }
            // fields[0] is now the volume/snap name, and fields[1] is the mountpoint.
            let vols: Vec<_> = fields[0].splitn(2, '@').collect();
            match vols.len() {
                1 => builder.push_volume(vols[0], fields[1]),
                2 => builder.push_snap(vols[0], vols[1]),
                _ => panic!("Unexpected zfs output"),
            }
        }
        let result = builder.into_sets();
        // println!("snaps: {:#?}", result);
        Ok(result)
    }

    /// For all snapshots, find the highest numbered dataset.
    pub fn next_snap(&self, sets: &[DataSet]) -> u32 {
        let mut next = 0u32;
        for ds in sets {
            for sn in &ds.snaps {
                match self.snap_re.captures(sn) {
                    None => (),
                    Some(caps) => {
                        let num = caps.at(1).unwrap().parse::<u32>().unwrap();
                        if num > next {
                            next = num;
                        }
                    },
                }
            }
        }
        next + 1
    }

    /// Take the next snapshot.
    pub fn take_snapshot(&self) -> Result<()> {
        let snaps = try!(self.get_snaps(self.base()));
        let num = self.next_snap(&snaps);
        let today = Local::today();
        let name = format!("{}@{}{:05}-{:02}-{:02}", self.base(),
                           self.back.host.snap_prefix, num,
                           today.month(), today.day());

        let mut cmd = Command::new("zfs");
        cmd.args(&["snapshot", "-r", &name]);
        if self.back.dry_run {
            println!("Would run: {:?}", cmd);
        } else {
            println!("Run: {:?}", cmd);
            let stat = try!(cmd.status());
            if !stat.success() {
                return Err(format!("Unable to run zfs command: {:?}", stat).into());
            }
        }
        Ok(())
    }

    /// Get all of the filesystems we care about, and update 'sure' data for all of them.
    pub fn run_sure(&self) -> Result<()> {
        let base = self.base();

        let snaps = try!(self.get_snaps(base));
        let snaps: Vec<_> = snaps.iter().filter(|x| x.name != base && !x.name.ends_with("/sure")).collect();
        // println!("sure: {:#?}", snaps);

        for ds in snaps {
            println!("Run sure on {:?} at {}", ds.name, ds.mount);

            let mut last = None;
            let subname = &ds.name[base.len()+1..];
            // println!("  sub: {:?}", subname);
            for snap in &ds.snaps {
                let name = format!("/{}/sure/{}-{}.dat.gz", base, subname, snap);
                if Path::new(&name).is_file() {
                    last = Some(name);
                    continue;
                }

                // println!("  {:?}", name);
                let dir = format!("{}/.zfs/snapshot/{}", ds.mount, snap);

                // The zfs snapshot automounter is a bit peculiar.  To ensure the directory is
                // actually mounted, run a command in that directory.
                try!(self.ensure_dir(&dir));

                match last {
                    None => try!(self.full_sure(&dir, &name)),
                    Some(ref old_name) => try!(self.incremental_sure(&dir, old_name, &name)),
                }
                if self.back.dry_run {
                } else {
                }

                last = Some(name);

            }
        }
        Ok(())
    }

    fn ensure_dir(&self, dir: &str) -> Result<()> {
        let mut cmd = Command::new("pwd");
        cmd.current_dir(dir);
        let stat = try!(cmd.status());
        if !stat.success() {
            return Err(format!("Unable to run pwd command in snapshot dir {:?}", stat).into());
        }
        Ok(())
    }

    fn full_sure(&self, dir: &str, name: &str) -> Result<()> {
        println!("  % sure -f {} ({})", name, dir);
        if !self.back.dry_run {
            try!(rsure::update(dir, rsure::no_path(), name));
        }
        Ok(())
    }

    fn incremental_sure(&self, dir: &str, old_name: &str, name: &str) -> Result<()> {
        println!("  % sure --old {} -f {} ({})", old_name, name, dir);
        if !self.back.dry_run {
            try!(rsure::update(dir, Some(old_name), name));
        }
        Ok(())
    }

    fn base(&self) -> &str {
        &self.back.host.base[..]
    }

    pub fn prune_snaps(&self) -> Result<()> {
        let snaps = try!(self.get_snaps(self.base()));
        for ds in &snaps {
            println!("name: {}", ds.name);
            let mut seen = HashMap::new();
            let mut prunes = Vec::new();
            for snap in &ds.snaps {
                match self.snap_re.captures(snap) {
                    None => (),
                    Some(caps) => {
                        let num = caps.at(1).unwrap().parse::<u32>().unwrap();
                        seen.insert(num, PruneInfo {
                            num: num,
                            name: snap.to_owned(),
                        });

                        // Prune away entries with the same number of bits.
                        let mypop = num.count_ones();
                        for i in 1 .. num {
                            if i.count_ones() != mypop {
                                continue
                            }
                            match seen.entry(i) {
                                Entry::Occupied(ent) => {
                                    prunes.push(ent.remove());
                                },
                                Entry::Vacant(_) => (),
                            }
                        }
                    },
                }
            }

            // Prune the old ones, but make sure to keep some.
            if prunes.len() > PRUNE_KEEP {
                for prune in &prunes[..prunes.len() - PRUNE_KEEP] {
                    let name = format!("{}@{}", ds.name, prune.name);
                    let mut cmd = Command::new("zfs");
                    cmd.arg("destroy");
                    cmd.arg(name);
                    println!(" % {:?}", cmd);
                    if !self.back.dry_run {
                        // TODO: Factor this always run command.
                        let stat = try!(cmd.status());
                        if !stat.success() {
                            return Err(format!("Unable to run zfs command: {:?}", stat).into());
                        }
                    }
                }
            }
        }

        return Ok(());
    }
}

#[derive(Debug)]
struct PruneInfo {
    num: u32,
    name: String,
}

#[derive(Debug)]
pub struct DataSet {
    name: String,
    snaps: Vec<String>,
    mount: String,
}

struct SnapBuilder {
    work: Vec<DataSet>,
}

impl SnapBuilder {
    fn new() -> SnapBuilder {
        SnapBuilder {
            work: vec![],
        }
    }

    fn into_sets(self) -> Vec<DataSet> {
        self.work
    }

    fn push_volume(&mut self, name: &str, mount: &str) {
        self.work.push(DataSet {
            name: name.to_owned(),
            snaps: vec![],
            mount: mount.to_owned(),
        });
    }

    fn push_snap(&mut self, name: &str, snap: &str) {
        let pos = self.work.len();
        if pos == 0 {
            panic!("Got snapshot from zfs before volume");
        }
        let set = &mut self.work[pos - 1];
        if name != set.name {
            panic!("Got snapshot from zfs without same volume name");
        }
        set.snaps.push(snap.to_owned());
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use sudo::Sudo;

    #[test]
    fn test_snaps() {
        let sudo = Sudo::new();
        let zfs = ZFS::new(&sudo, "arch/arch", "aa2015-");
        let snaps = zfs.get_snaps("a64/arch").unwrap();
        println!("next: {}", zfs.next_snap(&snaps));
    }
}
