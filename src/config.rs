//! Code for handling the config file.

use hostname;
use rustc_serialize::Decodable;
use std::borrow::Cow;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::result;
use toml;

pub type Result<T> = result::Result<T, Error>;

// Error type for parsing.
error_type! {
    #[derive(Debug)]
    pub enum Error {
        Io(io::Error) {
            cause;
        },
        TomlError(TomlError) {
            disp(e, fmt) write!(fmt, "{:?}", e);
            desc(_e) "Toml Parse Error";
        },
        Toml(toml::DecodeError) {
            cause;
        },
        Message(Cow<'static, str>) {
            desc(e) &**e;
            from(s: &'static str) s.into();
            from(s: String) s.into();
        },
        UnknownHost(UnknownHost) {
            disp(e, fmt) write!(fmt, "Unknown host: {:?}", e.0);
            desc(_e) "Unknown host";
        }
    }
}

#[derive(Debug)]
pub struct TomlError;

#[derive(Debug)]
pub struct UnknownHost(String);

// The top level of the config file are the host entries.  The key isn't really
// important, and just serves to group the entries together.

#[derive(Debug, RustcDecodable)]
pub struct Host {
    pub host: String,
    pub base: String,
    pub snap_prefix: String,
}

#[derive(Debug)]
pub struct ConfigFile(Vec<Host>);

impl Host {
    /// Retrieve the config file from the toml file at the given path.
    pub fn load<P: AsRef<Path>>(name: P) -> Result<ConfigFile> {
        let mut text = String::new();
        let mut f = try!(File::open(name));
        try!(f.read_to_string(&mut text));

        let tml = match toml::Parser::new(&text).parse() {
            Some(stuff) => stuff,
            None => return Err(Error::TomlError(TomlError)),
        };

        let mut result = vec![];

        for v in tml.values() {
            // TODO: Can we do with a move instead of a clone.
            // println!("{:?}", v);
            let mut dec = toml::Decoder::new(v.clone());
            let host: Host = try!(Decodable::decode(&mut dec));
            // println!("{:?}", host);
            result.push(host);
        }

        Ok(ConfigFile(result))
    }
}

impl ConfigFile {
    pub fn lookup(&self) -> Result<&Host> {
        let host = try!(hostname::get());
        for ent in &self.0 {
            if ent.host == host {
                return Ok(&ent);
            }
        }
        return Err(Error::UnknownHost(UnknownHost(host)));
    }
}
