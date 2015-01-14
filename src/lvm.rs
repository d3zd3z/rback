// Extracting information from lvm.

use std::collections::BTreeMap;
use std::error;
use std::io;
use std::io::process;
use std::iter;

use sudo::Sudoer;

#[derive(Show)]
pub enum Error {
    Io(io::IoError),
    Command(process::ProcessExit),
    Message(String),
}

impl Error {
    fn message(text: &str) -> Error {
        Error::Message(text.to_string())
    }
}

impl error::FromError<io::IoError> for Error {
    fn from_error(err: io::IoError) -> Error {
        Error::Io(err)
    }
}

#[derive(Show)]
pub struct LvmInfo {
    pub entries: Vec<LvmEntry>,
}

impl LvmInfo {
    pub fn get<T: Sudoer>(sudo: &T) -> Result<LvmInfo, Error> {
        let mut cmd = sudo.cmd("lvs");
        cmd.args(&["--separator", "|"]);

        let output = try!(cmd.output());
        if output.status != process::ExitStatus(0) {
            return Err(Error::Command(output.status));
        }

        if output.error.len() > 0 && log_enabled!(::log::WARN) {
            let text = String::from_utf8_lossy(output.error.as_slice());
            for line in text.lines() {
                warn!("lvm: {}", line);
            }
            warn!("stderr messages from lvm command: {}", text);
        }

        let text = String::from_utf8_lossy(output.output.as_slice());

        let mut lines = text.lines();

        let dec = match lines.next() {
            None => return Err(Error::Message("lvm had no header line".to_string())),
            Some(hd) => try!(LvmDecoder::new(hd)),
        };

        let mut items = vec!();

        for line in lines {
            items.push(try!(dec.decode(line)));
        }

        // Sort the items so that the names will present in order.
        items.sort();

        Ok(LvmInfo { entries: items })
    }
}

#[derive(Show, Eq, Ord, PartialEq, PartialOrd)]
pub struct LvmEntry {
    pub lv: String,
    pub vg: String,
}

struct LvmDecoder {
    lv_pos: uint,
    vg_pos: uint,
}

impl LvmDecoder {
    fn new(header: &str) -> Result<LvmDecoder, Error> {
        let mut result = BTreeMap::new();

        for (field, i) in try!(LvmDecoder::ltrim(header)).split('|').zip(iter::count(0u, 1)) {
            match result.insert(field.to_string(), i) {
                None => (),
                Some(i2) => {
                    debug!("Duplicate lvm key: {}, at {} and {}", field, i, i2);
                    return Err(Error::message("Duplicate key in LVM output"))
                }
            }
            result[field.to_string()] = i;
        }

        Ok(LvmDecoder {
           lv_pos: try!(LvmDecoder::find_field(&result, "LV")),
           vg_pos: try!(LvmDecoder::find_field(&result, "VG")),
        })
    }

    // Decode a single line.
    fn decode(&self, line: &str) -> Result<LvmEntry, Error> {
        let line = try!(LvmDecoder::ltrim(line));
        let fields: Vec<_> = line.split('|').collect();
        Ok(LvmEntry {
           lv: fields[self.lv_pos].to_string(),
           vg: fields[self.vg_pos].to_string(),
       })
    }

    // Attempt to trim the two spaces off of the front of an lvm line.
    fn ltrim<'a>(line: &'a str) -> Result<&'a str, Error> {
        if line.len() < 3 {
            return Err(Error::message("LVM input line too short"));
        }

        if !line.starts_with("  ") {
            return Err(Error::message("LVM input line doesn't start with two spaces"));
        }

        Ok(line.slice_from(2))
    }

    // Try to find the field in the given mapping.
    fn find_field(map: &BTreeMap<String, uint>, name: &str) -> Result<uint, Error> {
        map.get(name)
            .map_or_else(|| Err(Error::message(format!("missing key from LVM: {}", name).as_slice())),
                         |&x| Ok(x))
    }
}

#[cfg(test)]
mod test {
    use super::{ LvmInfo, LvmEntry };
    use sudo::FakeSudo;
    use std::io::File;

    // Compare the output of the above LVM parser against a simpler sanitized version.
    #[test]
    fn test_lvm() {
        let sudo = FakeSudo::new("tests/fake-lvm.sh");
        let info = LvmInfo::get(&sudo).unwrap();

        let rd = File::open(&Path::new("tests/fake-lvm.good")).unwrap().read_to_string().unwrap();
        let expect: Vec<_> = rd.lines().map(|line| {
            let fields: Vec<_> = line.split('|').collect();
            LvmEntry {
                lv: fields[0].to_string(),
                vg: fields[1].to_string(),
            }
        }).collect();

        assert_eq!(info.entries, expect);
    }
}
