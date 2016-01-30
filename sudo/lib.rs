// Enhance commands to run inside sudo

#[macro_use] extern crate lazy_static;
extern crate libc;
extern crate schedule_recv;

use schedule_recv::periodic_ms;
use std::ffi::OsStr;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

pub enum Sudo {
    // Used when we are already root.
    NoSudo,
    Sudo {
        ticker: JoinHandle<()>,
        count: Arc<Mutex<u64>>,
    },
}

impl Sudo {
    pub fn new() -> Sudo {
        Self::new_with_period(60000)
    }

    pub fn new_with_period(delay_ms: u32) -> Sudo {
        if *IS_ROOT {
            // If we're already root, don't do much.
            Sudo::NoSudo
        } else {
            let tick = periodic_ms(delay_ms);
            let count = Arc::new(Mutex::new(0));
            let icount = count.clone();
            let ticker = thread::spawn(move || {
                loop {
                    tick.recv().unwrap();
                    sudo_update();
                    *icount.lock().unwrap() += 1;
                }
            });
            Sudo::Sudo {
                ticker: ticker,
                count: count,
            }
        }
    }

    /// Construct a new command, like Command::new(), but, if sudo is needed, set the new command
    /// up to invoke with sudo.
    pub fn cmd<S: AsRef<OsStr>>(self, program: S) -> Command {
        match self {
            Sudo::NoSudo => Command::new(program),
            Sudo::Sudo { .. } => {
                let mut cmd = Command::new("sudo");
                cmd.arg(program);
                cmd
            }
        }
    }
}

// Run a single 'sudo -v' to make sure we can properly be root.  This command is also useful to
// refresh the sudo timer, so the user won't unexpectedly be prompted for the password.
fn sudo_update() {
    let mut cmd = Command::new("sudo");
    cmd.arg("-v")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    match cmd.status() {
        Ok(status) if status.success() => (),
        Ok(status) => panic!("Error running sudo -v: {}", status),
        Err(e) => panic!("Failed to execute sudo -v: {}", e),
    }
}

lazy_static! {
    static ref IS_ROOT: bool = {
        use libc::geteuid;

        unsafe { geteuid() == 0 }
    };
}

#[cfg(test)]
mod test {
    use std::env;
    use std::thread;
    use std::time::Duration;
    use super::{IS_ROOT, sudo_update, Sudo};

    #[test]
    fn not_root() {
        if *IS_ROOT {
            panic!("Tests should not be running as root");
        }
    }

    #[test]
    fn run_update() {
        sudo_update();
    }

    #[test]
    fn runs_as_root() {
        let sudo = Sudo::new();

        let mut cmd = sudo.cmd("id");
        cmd.arg("-u");
        let text = match cmd.output() {
            Ok(ref out) if !out.status.success() => panic!("Error with command {:?}", out.status),
            Ok(out) => out.stdout,
            Err(e) => panic!("Unable to run 'id' command: {:?}", e),
        };
        if text != b"0\n" {
            panic!("Unexpected user id, expecting 0: {:?}", text);
        }
    }

    #[test]
    fn bg_update() {
        // Normally not run, because it takes a while.
        // Run if 'SLOW_TESTS' is set in the environment.

        if env::var_os("SLOW_TESTS").is_some() {
            let sudo = Sudo::new_with_period(100);
            thread::sleep(Duration::from_secs(2));

            match sudo {
                Sudo::NoSudo => (),
                Sudo::Sudo { count, .. } => {
                    let count = *count.lock().unwrap();
                    if count < 15 || count > 30 {
                        panic!("Count isn't appropriate {}", count);
                    }
                }
            }
        }
    }
}
