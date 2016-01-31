// ZFS support

use chrono::{Datelike, Local};
use regex::{self, Regex};
use std::error;
use std::io::prelude::*;
use std::io::{BufReader};
use std::result;
use sudo::Sudo;

// For dev, boxed errors.
pub type Error = Box<error::Error + Send + Sync>;
pub type Result<T> = result::Result<T, Error>;

pub struct ZFS<'a> {
    sudo: &'a Sudo,
    snap_re: Regex,
    base: String,
    prefix: String,
}

impl<'a> ZFS<'a> {
    pub fn new<'b>(sudo: &'b Sudo, base: &str, snap_prefix: &str) -> ZFS<'b> {
        let quoted = regex::quote(snap_prefix);
        let pat = format!("^{}(\\d+)[-\\.]([-\\.\\d]+)$", quoted);
        ZFS {
            sudo: sudo,
            snap_re: Regex::new(&pat).unwrap(),
            base: base.to_owned(),
            prefix: snap_prefix.to_owned(),
        }
    }

    pub fn get_snaps(&self, dir: &str) -> Result<Vec<DataSet>> {
        let mut cmd = self.sudo.cmd("zfs");
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
        let snaps = try!(self.get_snaps(&self.base));
        let num = self.next_snap(&snaps);
        let today = Local::today();
        let name = format!("{}@{}{:05}-{:02}-{:02}", self.base, self.prefix, num,
                           today.month(), today.day());

        let mut cmd = self.sudo.cmd("zfs");
        cmd.args(&["snapshot", "-r", &name]);
        let stat = try!(cmd.status());
        if !stat.success() {
            return Err(format!("Unable to run zfs command: {:?}", stat).into());
        }
        Ok(())
    }
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
