// Enhance commands to run inside sudo

extern crate libc;

use std::io::Command;
use std::io::process;
use std::io::process::StdioContainer;
use std::thread::{JoinGuard, Thread};
use std::io::timer::Timer;
use std::time::Duration;

// Since the command isn't very accessible...  Use this to create a new command for building.

pub enum Sudo {
    // When there is no sudo needed (already root).
    NoSudo,
    Sudo {
        keeper: JoinGuard<()>,
        ticker: Timer,
        stop: std::comm::Sender<()>
    },
}

impl Sudo {
    /// Construct a new sudo manager.  If necessary, it will run "sudo -v" to make sure the
    /// password has been entered, and will periodically run this command to make sure that sudo
    /// doesn't consider itself idle.
    pub fn new() -> Sudo {
        Sudo::new_with_period(Duration::seconds(60))
    }

    pub fn new_with_period(delay: Duration) -> Sudo {
        if is_root() {
            // If we're already root, don't do much.
            Sudo::NoSudo
        } else {
            sudo_update();

            let mut ticker = Timer::new().unwrap();
            let msg = ticker.periodic(delay);
            let (tx, rx) = std::comm::channel();
            let child = Thread::spawn(move || {

                loop {
                    select! (
                        () = msg.recv() => sudo_update(),
                        () = rx.recv() => break
                    )
                }
                // println!("Child leaving");
            });

            Sudo::Sudo {
                keeper: child,
                ticker: ticker,
                stop: tx
            }
        }
    }

    /// Construct a new command, like Command::new(), but, if sudo is needed, set the new command
    /// up to invoke with sudo.
    pub fn cmd<T: ToCStr>(&self, program: T) -> Command {
        match self {
            &Sudo::NoSudo => Command::new(program),
            &Sudo::Sudo { .. } => {
                let mut cmd = Command::new("sudo");
                cmd.arg(program);
                cmd
            }
        }
    }
}

// Need Drop to be able to tell the child to terminate.
impl Drop for Sudo {
    fn drop(&mut self) {
        match self {
            &Sudo::NoSudo => (),
            &Sudo::Sudo { ref stop, ..} => {
                stop.send(());
            },
        }
    }
}

// Determine if we are currently running as root.  Static initializers are a bit in flux, so just
// re-compute this each time.  It is a syscall, but shouldn't be too bad.
fn is_root() -> bool {
    use libc::funcs::posix88::unistd::geteuid;

    unsafe { geteuid() == 0 }
}

// Run a 'sudo -v' to make sure that we are able to properly be root.
fn sudo_update() {
    // println!("sudo-update");
    let mut cmd = Command::new("sudo");
    cmd.arg("-v")
        .stdin(StdioContainer::InheritFd(0))
        .stdout(StdioContainer::InheritFd(1))
        .stderr(StdioContainer::InheritFd(2));

    match cmd.status() {
        Ok(process::ExitStatus(0)) => (),
        Ok(status) => panic!("Error running sudo -v: {}", status),
        Err(e) => panic!("Failed to execute sudo -v: {}", e),
    }
}

#[cfg(test)]
mod test {
    use std::str;
    use std::io::process::{ExitStatus, StdioContainer};
    use std::time::Duration;
    use std::io::timer::sleep;

    use super::is_root;

    // TODO: Rust runs these in parallel, which results in multiple probes for the password.

    #[test]
    fn update_sudo() {
        println!("Start update");
        let sudo = super::Sudo::new_with_period(Duration::milliseconds(200));
        sleep(Duration::seconds(1));
        println!("Stopping update");
        drop(sudo);
        println!("Stopped");
    }

    #[test]
    fn not_as_root() {
        assert!(!is_root());
    }

    #[test]
    fn simple_run() {
        let sudo = super::Sudo::new();
        let mut cmd = sudo.cmd("id");
        cmd.stdin(StdioContainer::Ignored);

        let out = cmd.output().unwrap();
        assert!(out.status == ExitStatus(0));
        let text = str::from_utf8(out.output.as_slice()).unwrap();
        assert!(text.starts_with("uid=0("));
    }
}
