use std::sync::Arc;

pub use subprocess_impl::SbProcess::{self as Process};

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct ProcessId {
    id: u32,
    command: Arc<str>,
}

impl ProcessId {
    pub fn new(id: u32, command: String) -> Self {
        Self {
            id,
            command: command.into_boxed_str().into(),
        }
    }
    pub fn command(&self) -> &str {
        &self.command
    }
}

impl std::fmt::Display for ProcessId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[{}]: {}", self.id, self.command)
    }
}

#[derive(Debug, Clone)]
pub enum ProcessSignal {
    SIGINT,
    SIGTERM,
    SIGKILL,
}

#[derive(Clone, Copy)]
pub enum ProcessStdio {
    Inherit,
    Raw,
    StderrOnly,
}

impl From<bool> for ProcessStdio {
    fn from(b: bool) -> Self {
        if b {
            Self::Raw
        } else {
            Self::Inherit
        }
    }
}

mod subprocess_impl {
    use std::{
        io::BufRead,
        sync::{Arc, RwLock},
    };

    use subprocess::{ExitStatus, Popen, PopenConfig};

    use crate::{
        errors::{TogetherInternalError, TogetherResult},
        log, log_err,
    };

    use super::{ProcessId, ProcessSignal, ProcessStdio};

    pub struct SbProcess {
        popen: subprocess::Popen,
        mute: Option<Arc<RwLock<bool>>>,
    }

    impl SbProcess {
        pub fn spawn(
            command: &str,
            cwd: Option<&str>,
            stdio: ProcessStdio,
        ) -> TogetherResult<Self> {
            let mut config = PopenConfig::default();
            config.stdout = match stdio {
                ProcessStdio::Raw => subprocess::Redirection::None,
                _ => subprocess::Redirection::Pipe,
            };
            config.stderr = match stdio {
                ProcessStdio::Raw | ProcessStdio::StderrOnly => subprocess::Redirection::None,
                _ => subprocess::Redirection::Pipe,
            };
            config.cwd = cwd.map(|s| s.into());

            #[cfg(unix)]
            {
                config.setpgid = true;
            }

            let mut argv = os::SHELL.to_vec();
            argv.push(command);
            let popen = Popen::create(&argv, config)?;
            let mute = Arc::new(RwLock::new(false));

            Ok(Self {
                popen,
                mute: Some(mute),
            })
        }

        pub fn kill(&mut self, signal: Option<&ProcessSignal>) -> TogetherResult<()> {
            fn check_err<T: Ord + Default>(num: T) -> std::io::Result<T> {
                if num < T::default() {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(num)
            }

            #[cfg(windows)]
            {
                Ok(self.popen.terminate()?)
            }
            #[cfg(unix)]
            {
                self.popen.poll();
                let pid = match self.popen.pid() {
                    Some(pid) => pid as i32,
                    _ => return Ok(()),
                };
                let signal = match signal {
                    Some(ProcessSignal::SIGINT) => libc::SIGINT,
                    Some(ProcessSignal::SIGTERM) => libc::SIGTERM,
                    Some(ProcessSignal::SIGKILL) => libc::SIGKILL,
                    None => libc::SIGTERM,
                };
                let _code = check_err(unsafe { libc::kill(-pid, signal) })?;
                Ok(())
            }
        }

        pub fn try_wait(&mut self) -> TogetherResult<Option<i32>> {
            match self.popen.poll() {
                Some(ExitStatus::Exited(code)) => Ok(Some(code as i32)),
                Some(ExitStatus::Signaled(_)) => Ok(Some(1)),
                Some(ExitStatus::Other(_)) | Some(ExitStatus::Undetermined) => {
                    Err(TogetherInternalError::ProcessFailedToExit.into())
                }
                None => Ok(None),
            }
        }

        pub fn forward_stdio(&mut self, id: &ProcessId) {
            let stdout = self.popen.stdout.take().unwrap();
            let stderr = self.popen.stderr.take().unwrap();
            let id = id.clone();
            let mute = self.mute.clone();
            std::thread::spawn(move || {
                let id = id.clone();
                Self::forward_stdio_blocking(&id, stdout, stderr, mute)
            });
        }

        fn forward_stdio_blocking(
            id: &ProcessId,
            stdout: std::fs::File,
            stderr: std::fs::File,
            mute: Option<Arc<RwLock<bool>>>,
        ) {
            let mut stdout = std::io::BufReader::new(stdout);
            let mut stderr = std::io::BufReader::new(stderr);
            let mut stdout_line = String::new();
            let mut stderr_line = String::new();
            loop {
                let mut stdout_done = false;
                let mut stderr_done = false;
                let mut stdout_bytes = vec![];
                let mut stderr_bytes = vec![];
                let stdout_read = stdout.read_line(&mut stdout_line);
                let stderr_read = stderr.read_line(&mut stderr_line);
                match (stdout_read, stderr_read) {
                    (Ok(0), Ok(0)) => {
                        stdout_done = true;
                        stderr_done = true;
                    }
                    (Ok(0), _) => {
                        stdout_done = true;
                    }
                    (_, Ok(0)) => {
                        stderr_done = true;
                    }
                    (Ok(_), Ok(_)) => {}
                    (Err(e), _) => {
                        log_err!("Failed to read stdout: {}", e);
                        stdout_done = true;
                    }
                    (_, Err(e)) => {
                        log_err!("Failed to read stderr: {}", e);
                        stderr_done = true;
                    }
                }
                if !stdout_done {
                    stdout_bytes.extend(stdout_line.as_bytes());
                    stdout_line.clear();
                }
                if !stderr_done {
                    stderr_bytes.extend(stderr_line.as_bytes());
                    stderr_line.clear();
                }
                if !stdout_bytes.is_empty() {
                    while mute.as_ref().map_or(false, |m| *m.read().unwrap()) {
                        log!("Skipping muted process {}", id.id);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    print!("{}: {}", id.id, String::from_utf8_lossy(&stdout_bytes));
                }
                if !stderr_bytes.is_empty() {
                    eprint!("{}: {}", id.id, String::from_utf8_lossy(&stderr_bytes));
                }
                if stdout_done && stderr_done {
                    break;
                }
            }
        }
    }

    #[cfg(unix)]
    mod os {
        pub const SHELL: [&str; 2] = ["sh", "-c"];
    }

    #[cfg(windows)]
    mod os {
        pub const SHELL: [&str; 2] = ["cmd.exe", "/c"];
    }
}
