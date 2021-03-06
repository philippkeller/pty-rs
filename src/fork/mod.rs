mod pty;
mod err;

use ::descriptor::Descriptor;

use ::libc;
pub use self::err::{ForkError, Result};
pub use self::pty::{Master, MasterError};
pub use self::pty::{Slave, SlaveError};
use std::ffi::CString;

#[derive(Debug)]
pub enum Fork {
    // Parent child's pid and master's pty.
    Parent(libc::pid_t, Master),
    // Child pid 0.
    Child(Slave),
}

impl Fork {
    /// The constructor function `new` forks the program
    /// and returns the current pid.
    pub fn new(path: &'static str) -> Result<Self> {
        match Master::new(CString::new(path).ok().unwrap_or_default().as_ptr()) {
            Err(cause) => Err(ForkError::BadMaster(cause)),
            Ok(master) => unsafe {
                if let Some(cause) = master.grantpt().err().or(master.unlockpt().err()) {
                    Err(ForkError::BadMaster(cause))
                } else {
                    match libc::fork() {
                        -1 => Err(ForkError::Failure),
                        0 => {
                            match master.ptsname() {
                                Err(cause) => Err(ForkError::BadMaster(cause)),
                                Ok(name) => Fork::from_pts(name),
                            }
                        }
                        pid => Ok(Fork::Parent(pid, master)),
                    }
                }
            },
        }
    }

    /// The constructor function `from_pts` is a private
    /// extension from the constructor function `new` who
    /// prepares and returns the child.
    fn from_pts(ptsname: *const ::libc::c_char) -> Result<Self> {
        unsafe {
            // make parent process the session leader
            // so e.g. Ctrl-C is sent to the slave
            if libc::setsid() == -1 {
                Err(ForkError::SetsidFail)
            } else {
                match Slave::new(ptsname) {
                    Err(cause) => Err(ForkError::BadSlave(cause)),
                    Ok(slave) => {
                        slave.dup2(libc::STDIN_FILENO)
                            .and_then(|_| slave.dup2(libc::STDOUT_FILENO))
                            .and_then(|_| slave.dup2(libc::STDERR_FILENO))
                            .and_then(|_| Ok(Fork::Child(slave)))
                            .or_else(|e| Err(ForkError::BadSlave(e)))
                    }
                }
            }
        }
    }

    /// The constructor function `from_ptmx` forks the program
    /// and returns the current pid for a default PTMX's path.
    pub fn from_ptmx() -> Result<Self> {
        Fork::new(::DEFAULT_PTMX)
    }

    /// Waits until slave is terminated (blocking call)
    /// Returns exit status of slave process
    pub fn wait(&self) -> Result<(libc::c_int)> {
        match *self {
            Fork::Child(_) => Err(ForkError::IsChild),
            Fork::Parent(pid, _) => {
                let mut status = 0;
                loop {
                    unsafe {
                        match libc::waitpid(pid, &mut status, 0) {
                            0 => continue,
                            -1 => return Err(ForkError::WaitpidFail),
                            _ => return Ok(status),
                        }
                    }
                }
            }
        }
    }

    /// The function `is_parent` returns the pid or parent
    /// or none.
    pub fn is_parent(&self) -> Result<Master> {
        match *self {
            Fork::Child(_) => Err(ForkError::IsChild),
            Fork::Parent(_, ref master) => Ok(master.clone()),
        }
    }

    /// The function `is_child` returns the pid or child
    /// or none.
    pub fn is_child(&self) -> Result<&Slave> {
        match *self {
            Fork::Parent(_, _) => Err(ForkError::IsParent),
            Fork::Child(ref slave) => Ok(slave),
        }
    }
}

impl Drop for Fork {
    fn drop(&mut self) {
        match *self {
            Fork::Parent(_, ref master) => Descriptor::drop(master),
            _ => {}
        }
    }
}
