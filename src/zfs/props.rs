//! Manage ZFS properties
//!
//! Parse and read the output of 'zfs get' to be able to interpret those that are meaningful.

use std::io::prelude::*;
use std::io::BufReader;
use super::{local_path, DataSet, Result, ZFS};

impl<'a> ZFS<'a> {
    /// Read the ZFS properties for the given `DataSet`.  This runs the "zfs get" command, and
    /// parses the output.
    pub fn get_props(&self, ds: &DataSet, snap: Option<&str>) -> Result<PropSet> {
        let mut cmd = ds.dir.command();

        let dname = snap.map_or_else(|| ds.name.to_owned(),
            |v| format!("{}@{}", ds.name, v));
        println!("get: {:?}", dname);
        cmd.args(&["get", "-Hp", "all", &dname]);
        let out = cmd.output()?;
        if !out.status.success() {
            return Err(format!("zfs get returned error: {:?}", out.status).into());
        }

        let mut result = vec![];

        let buf = out.stdout;
        for line in BufReader::new(&buf[..]).lines() {
            let line = line?;
            let fields: Vec<_> = line.splitn(4, '\t').collect();
            if fields.len() != 4 {
                return Err(format!("zfs line doesn't have two fields: {:?}", line).into());
            }
            result.push(Prop::new(fields[1], fields[2], fields[3]));
        }
        Ok(PropSet {
            props: result,
        })
    }

    /// Debugging entry point, show the props for the specified subvolumes.
    pub fn show_props(&self) -> Result<()> {
        let dss = self.get_snaps(local_path(&self.base()))?;
        println!("There are {} datasets", dss.len());
        for ds in &dss {
            // Get the parent properties.
            println!("Props for {:?}", ds.name);
            // let ps = self.get_props(ds, ds.snaps.first().map(|v| v.as_str()))?;
            let ps = self.get_props(ds, None)?;
            println!("  mounted   : {:?}", ps.is_mounted());
            println!("  mountpoint: {:?}", ps.mountpoint());
            // println!("  {:?}", self.get_props(ds)?);
        }
        Ok(())
    }
}

/// A property set holds a set of properties, and has convenient ways of searching for specific
/// kinds of values.
#[derive(Debug)]
pub struct PropSet {
    props: Vec<Prop>,
}

impl PropSet {
    /// Determine if this filesystem is mounted.  None means the property wasn't present.
    pub fn is_mounted(&self) -> Option<bool> {
        self.scan_name("mounted").and_then(|x| PropSet::from_yesno(&x.value))
    }

    /// Return the mount point of this filesystem.
    pub fn mountpoint(&self) -> Option<&str> {
        self.scan_name("mountpoint").map(|x| x.value.as_str())
    }

    /// Scan for a property of the given name, and return it if found.
    fn scan_name(&self, name: &str) -> Option<&Prop> {
        for p in &self.props {
            if p.name == name {
                return Some(p);
            }
        }
        None
    }

    /// Decode a yes/no response
    fn from_yesno(text: &str) -> Option<bool> {
        match text {
            "yes" => Some(true),
            "no" => Some(false),
            _ => None,
        }
    }
}

/// A property has a name, a value, an a source.  For now, let's just store all of these are
/// strings, and decode them on demand.  We don't store the path for the property, since that can be
/// inferred for how the property was obtained.  There will be lots of redundant strings here,
/// because of lots of common values.
#[derive(Debug)]
pub struct Prop {
    name: String,
    value: String,
    origin: String,
}

impl Prop {
    pub fn new(name: &str, value: &str, origin: &str) -> Prop {
        Prop {
            name: name.to_owned(),
            value: value.to_owned(),
            origin: origin.to_owned(),
        }
    }
}
