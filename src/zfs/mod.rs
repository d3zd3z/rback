// ZFS support

use chrono::{Datelike, Local};
use regex::{self, Regex};
use rsure::{self, Progress, SureHash, TreeUpdate};
use rsure::bk::BkDir;
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use std::path::Path;
use std::io::prelude::*;
use std::io::{self, BufReader};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::process::{Command, Stdio};
use std::string;

error_chain! {
    types {
        Error, ErrorKind, ChainErr, Result;
    }

    links {
        rsure::Error, rsure::ErrorKind, Rsure;
    }

    foreign_links {
        io::Error, IoError;
        string::FromUtf8Error, Utf8Error;
    }

    errors {
    }
}

use RBack;

// For pruning, always keep at least this many of the pruned snapshots.
const PRUNE_KEEP: usize = 10;

// A snap destination is somewhere that has a ZFS filesystem.
pub trait ZfsPath {
    /// Retrieve the local path name of this ZfsPath.  With no mount
    /// options, this will be the same as the directory name, but without a
    /// leading slash.
    fn name(&self) -> &str;

    /// Construct a zfs command to access this path.
    fn command(&self) -> Command;
}

impl ZfsPath {
    /// Parse the given path, returning a trait object for ZfsPath that is
    /// either local or remote depending on the user's desire.
    pub fn parse(text: &str) -> Box<ZfsPath> {
        match ZfsRemotePath::parse(text) {
            Some(zp) => Box::new(zp),
            None => Box::new(ZfsLocalPath(text.to_owned())),
        }
    }
}

/// The implementation for simple strings.  These will run the zfs commands
/// locally.
impl<'a> ZfsPath for &'a str {
    fn name(&self) -> &str {
        self
    }

    fn command(&self) -> Command {
        Command::new("zfs")
    }
}

/// An implementation for local hosts, with an owned string.
pub struct ZfsLocalPath(String);

impl ZfsPath for ZfsLocalPath {
    fn name(&self) -> &str {
        &self.0
    }

    fn command(&self) -> Command {
        Command::new("zfs")
    }
}

/// A remote path for Zfs.
pub struct ZfsRemotePath {
    /// The host (as given to ssh) that this path should be run on.
    host: String,
    /// The zfs path name itself.
    path: String,
}

impl ZfsRemotePath {
    // Parse a remote path, based on the first colon.  Returns `None` if
    // the given text does not have a colon.
    pub fn parse(text: &str) -> Option<ZfsRemotePath> {
        let parts: Vec<_> = text.splitn(2, ':').collect();
        if parts.len() != 2 {
            return None;
        }

        Some(ZfsRemotePath {
            host: parts[0].to_owned(),
            path: parts[1].to_owned(),
        })
    }
}

impl ZfsPath for ZfsRemotePath {
    fn name(&self) -> &str {
        &self.path
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new("ssh");
        cmd.args(&[&self.host[..], "zfs"]);
        cmd
    }
}

pub struct ZFS<'a> {
    back: &'a RBack,
    snap_re: Regex,
    send_size_re: Regex,
}

impl<'a> ZFS<'a> {
    pub fn new<'b>(back: &'b RBack) -> ZFS<'b> {
        let quoted = regex::quote(&back.host.snap_prefix);
        let pat = format!("^{}(\\d+)[-\\.]([-\\.\\d]+)$", quoted);
        ZFS {
            back: back,
            snap_re: Regex::new(&pat).unwrap(),
            send_size_re: Regex::new(r"(?s).*\nsize\t(\d+)\n$").unwrap(),
        }
    }

    pub fn get_snaps(&self, dir: &ZfsPath) -> Result<Vec<DataSet>> {
        let mut cmd = dir.command();
        cmd.args(&["list", "-H", "-t", "all", "-o", "name,mountpoint",
                 "-r", dir.name()]);
        let out = try!(cmd.output());
        if !out.status.success() {
            return Err(format!("zfs list returned error: {:?}", out.status).into());
        }
        let buf = out.stdout;
        // println!("Len: {} bytes", buf.len());

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

    // Get the list of snaps, but eliminate those related to surefiles.
    fn get_nonsure_snaps(&self, dir: &str) -> Result<Vec<DataSet>> {
        Ok(try!(self.get_snaps(&dir))
           .into_iter()
           .filter(|x| x.name != dir &&
                   !x.name.ends_with("/sure") &&
                   !x.name.ends_with("/bksure"))
           .collect())
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
        let snaps = try!(self.get_snaps(&self.base()));
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

        let snaps = try!(self.get_nonsure_snaps(base));

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

    /// Update sure for all filesystems we care about, and update the
    /// 'sure' data within a bksure store.
    pub fn run_bksure(&self) -> Result<()> {
        let base = self.base();
        let snaps = try!(self.get_nonsure_snaps(base));
        let bkd = try!(BkDir::new(&format!("/{}/bksure", base)));
        let present = try!(bkd.query());
        for ds in snaps {
            let mut last = None;
            let subname = &ds.name[base.len()+1..];
            let datname = format!("{}.dat", subname);
            let exists = present.iter()
                .filter(|x| x.file == datname)
                .map(|x| (&x.name[..]))
                .collect::<HashSet<_>>();
            // println!("  subname: {:?}", subname);
            // println!("  exists: {:#?}", exists);
            println!("Run bksure on {:?} at {}", ds.name, ds.mount);
            for snap in &ds.snaps {
                if exists.contains(&snap[..]) {
                    last = Some(snap.to_owned());
                    continue;
                }

                let dir = format!("{}/.zfs/snapshot/{}", ds.mount, snap);

                // The zfs snapshot automounter is a bit peculiar.  To
                // ensure the directory is actually mounted, run a command
                // in that directory.
                try!(self.ensure_dir(&dir));

                try!(self.bksure(&bkd, &dir, &datname, last.as_ref().map(|x| x.as_str()), &snap));
                last = Some(snap.clone());
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

    fn bksure(&self, bkd: &BkDir, dir: &str, file: &str, old: Option<&str>, name: &str) -> Result<()> {
        println!("  % sure file={:?} old={:?}, name={:?} (dir={:?})", file, old, name, dir);
        if !self.back.dry_run {
            // TODO: Generalize this functionality in rsure's API itself.
            let mut new_tree = try!(rsure::scan_fs(dir));

            // Update the hashes.
            match old {
                None => (),
                Some(src) => {
                    let old_tree = try!(bkd.load(file, src));
                    new_tree.update_from(&old_tree);
                },
            }

            let estimate = new_tree.hash_estimate();
            let mut progress = Progress::new(estimate.files, estimate.bytes);
            new_tree.hash_update(Path::new(dir), &mut progress);
            progress.flush();

            try!(bkd.save(&new_tree, file, name));
        }
        Ok(())
    }

    fn base(&self) -> &str {
        &self.back.host.base[..]
    }

    pub fn prune_snaps(&self) -> Result<()> {
        let snaps = try!(self.get_snaps(&self.base()));
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

    /// Clone the snapshots in 'src' to 'dest', going through each volume.
    pub fn clone_snaps(&self, src: &ZfsPath, dest: &ZfsPath) -> Result<()> {
        let state = CloneState {
            zfs: self,
            src: src,
            dest: dest,
        };
        state.clone_snaps()
    }

}

struct CloneState<'b, 'a: 'b, 'c, 'd> {
    src: &'c ZfsPath,
    dest: &'d ZfsPath,
    zfs: &'b ZFS<'a>,
}

impl<'a, 'b, 'c, 'd> CloneState<'a, 'b, 'c, 'd> {
    /// Clone the snapshots in 'src' to 'dest', going through each volume.
    fn clone_snaps(&self) -> Result<()> {
        let src_snaps = try!(self.zfs.get_snaps(self.src));
        let dest_snaps = try!(self.zfs.get_snaps(self.dest));

        // println!("src: {:#?}", src_snaps);
        // println!("dst: {:#?}", dest_snaps);

        // Build a mapping of the dest volumes, with the shared prefix stripped.
        let dmap: HashMap<_, _> =
            dest_snaps.iter().map(|e| (e.name[self.dest.name().len()..].to_owned(), e))
            .collect();

        // println!("dmap: {:#?}", dmap);

        for ssnap in &src_snaps {
            // println!("Check: {:?}", &ssnap.name[src.len()..]);
            match dmap.get(&ssnap.name[self.src.name().len()..]) {
                None => println!("Fresh: {}", ssnap.name),
                Some(dsnap) => {
                    println!("Clone: {}", ssnap.name);

                    try!(self.clone_volume(ssnap, dsnap));
                },
            }
        }

        Ok(())
    }

    fn clone_volume(&self, src: &DataSet, dest: &DataSet) -> Result<()> {
        // Scan for the most recent index in the src snapshots that is
        // present in the dests, and backup the rest.
        let dpresent = dest.snaps.iter().collect::<HashSet<_>>();
        let mut latest = None;
        for (i, sname) in src.snaps.iter().enumerate() {
            if dpresent.contains(sname) {
                latest = Some(i);
            }
        }

        let mut last = latest.clone();
        let first = latest.map(|x| x + 1).unwrap_or(0);
        for snum in first .. src.snaps.len() {
            let name = &src.snaps[snum];
            if dpresent.contains(name) {
                // This is already present.  Unsure if this should happen
                // as long as we're doing the backups.
                println!("Warning: snapshot is already present: {:?}", name);
            } else {
                let old_name = last.map(|x| &src.snaps[x][..]);
                println!("  clone {:?} {:?} to {:?} {:?}", src.name, old_name, dest.name, name);
                let size = try!(self.estimate_size(src, old_name, name));
                println!("    size: {:?}", size);
                try!(self.run_clone(src, dest, old_name, name, size));
            }

            last = Some(snum);

            // For development, stop after one clone to make sure it worked
            // right.
        }
        // println!("Latest: {:?}", last);

        Ok(())
    }

    fn estimate_size(&self, dset: &DataSet, old_name: Option<&str>, new_name: &str) -> Result<u64> {
        let mut cmd = self.src.command();
        cmd.args(&["send", "-nP", "-Le"]);
        match old_name {
            None => (),
            Some(name) => {
                cmd.args(&["-I", &format!("@{}", name)]);
            },
        }
        let new_arg = format!("{}@{}", dset.name, new_name);
        cmd.arg(&new_arg);
        let out = try!(cmd.output());
        if !out.status.success() {
            return Err(format!("zfs send returned error: {:?}", out.status).into());
        }
        let buf = out.stdout;
        let buf = try!(String::from_utf8(buf));
        // println!("Output: {} bytes {:?}", buf.len(), buf);

        match self.zfs.send_size_re.captures(&buf) {
            None => return Err(format!("zfs send didn't have size data").into()),
            Some(caps) => {
                Ok(caps.at(1).unwrap().parse::<u64>().unwrap())
            }
        }
    }

    fn run_clone(&self, src: &DataSet, dest: &DataSet,
                 old_name: Option<&str>, new_name: &str, est_size: u64) -> Result<()> {
        // TODO: A lot is common with `estimate_size`, factor that code
        // out.
        let mut cmd1 = self.src.command();
        cmd1.args(&["send", "-Le"]);
        match old_name {
            None => (),
            Some(name) => {
                cmd1.args(&["-I", &format!("@{}", name)]);
            },
        }
        let new_arg = format!("{}@{}", src.name, new_name);
        cmd1.arg(&new_arg);
        cmd1.stdout(Stdio::piped());
        let mut child1 = try!(cmd1.spawn());

        if self.zfs.back.dry_run {
            println!("ZFS clone: {:?} to {:?}@{:?}", old_name, src.name, new_name);
            return Ok(())
        }

        // Use the 'pv' program as a progress monitor.
        let mut cmd2 = Command::new("pv");
        let size_arg = format!("{}", est_size);
        cmd2.args(&["-s", &size_arg]);
        unsafe {
            let fd = child1.stdout.as_ref().unwrap().as_raw_fd();
            cmd2.stdin(Stdio::from_raw_fd(fd));
        }
        cmd2.stdout(Stdio::piped());
        cmd2.stderr(Stdio::inherit());
        let mut child2 = try!(cmd2.spawn());

        // Pipe this into zfs recv.
        let mut cmd3 = self.dest.command();
        cmd3.args(&["recv", "-vF", &dest.name]);
        unsafe {
            let fd = child2.stdout.as_ref().unwrap().as_raw_fd();
            cmd3.stdin(Stdio::from_raw_fd(fd));
        }
        cmd3.stdout(Stdio::inherit());
        cmd3.stderr(Stdio::inherit());
        let mut child3 = try!(cmd3.spawn());

        match try!(child1.wait()) {
            status if status.success() => (),
            status => {
                return Err(format!("Error running zfs send: {:?}", status).into());
            }
        }

        match try!(child2.wait()) {
            status if status.success() => (),
            status => {
                return Err(format!("Error running pv: {:?}", status).into());
            }
        }

        match try!(child3.wait()) {
            status if status.success() => (),
            status => {
                return Err(format!("Error running zfs recv: {:?}", status).into());
            }
        }

        Ok(())
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
    use config;
    use RBack;

    #[test]
    fn test_snaps() {
        let back = RBack {
            host: config::Host {
                host: "test-host".to_owned(),
                base: "arch/arch".to_owned(),
                snap_prefix: "aa2015-".to_owned(),
            },
            dry_run: false,
        };
        let zfs = ZFS::new(&back);
        let snaps = zfs.get_snaps(&"a64/arch").unwrap();
        println!("next: {}", zfs.next_snap(&snaps));
    }
}
