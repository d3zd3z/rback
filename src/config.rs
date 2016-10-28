//! Code for handling the config file.

use hostname;
use rustc_serialize::Decodable;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use toml;

// Error type for parsing.
error_chain! {
    types {
        Error, ErrorKind, ChainErr, Result;
    }

    links {
    }

    foreign_links {
        io::Error, IoError;
        toml::DecodeError, Toml;
    }

    errors {
        TomlError {
            description("Toml Parse Error")
            display("Error parsing Toml of config file")
        }
        UnknownHost(host: String) {
            description("Unknown host")
            display("Unknown host: {:?}", host)
        }
    }
}

// The top level of the config file are the host entries.  The key isn't really
// important, and just serves to group the entries together.

#[derive(Clone, Debug, RustcDecodable)]
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
            None => return Err(ErrorKind::TomlError.into()),
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
        return Err(ErrorKind::UnknownHost(host).into());
    }
}
