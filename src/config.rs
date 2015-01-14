//! Code for handling the config file.

use std::io;
use std::error;
use std::collections::BTreeMap;

use toml;
use toml::Decoder;

use rustc_serialize::Decodable;

// The top level of the config file are the host entries.  The key isn't
// really important, and really serves just to group entries together.
#[derive(Show)]
pub struct ConfigFile(Vec<Host>);

#[derive(RustcDecodable, Show)]
pub struct Host {
    pub host: String,
    pub snapdir: String,
    pub filesystems: Vec<FsInfo>,
    mirrors: Vec<BTreeMap<String, String>>,
}

#[derive(RustcDecodable, Show)]
pub struct FsInfo {
    volgroup: String,
    lvname: String,
    mount: String,
}

// Errors we can get.
#[derive(Show)]
pub enum Error {
    Io(io::IoError),
    Decode(toml::DecodeError),
    NoHost,
    Parse,
}

impl error::FromError<io::IoError> for Error {
    fn from_error(err: io::IoError) -> Error {
        Error::Io(err)
    }
}

impl error::FromError<toml::DecodeError> for Error {
    fn from_error(err: toml::DecodeError) -> Error {
        Error::Decode(err)
    }
}

// A Mirror has some simple operations on it.
pub trait Mirror {
}

impl Host {
    /// Retrieve the configuration information for the current host, if present.
    pub fn get_host(name: &Path, host: &str) -> Result<Host, Error> {
        let ConfigFile(conf) = try!(Host::load(name));

        for h in conf.into_iter() {
            if h.host.as_slice() == host {
                return Ok(h)
            }
        }

        Err(Error::NoHost)
    }

    /// the config file from the toml file at the given path.
    pub fn load(name: &Path) -> Result<ConfigFile, Error> {
        let text = {
            let mut f = try!(io::File::open(name));
            try!(f.read_to_string())
        };

        let mut parser = toml::Parser::new(text.as_slice());
        let toml = try!(parser.parse().ok_or(Error::Parse));

        let mut result = vec!();

        for (_k, v) in toml.into_iter() {
            let host = try!(Decodable::decode(&mut Decoder::new(v)));
            result.push(host);
        }

        Ok(ConfigFile(result))
    }

    /// Find an appropriate mirror type.
    pub fn get_mirror(&self, name: &str) -> Option<Box<Mirror>> {
        for v in self.mirrors.iter() {
            match v.get("name") {
                Some(n) if name == n.as_slice() => {
                    println!("Found: {:?}", v);
                    panic!("Found")
                },
                _ => continue,
            }
        }
        None
    }
}
