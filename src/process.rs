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

mod subprocess_impl {
    use std::{
        io::BufRead,
        sync::{Arc, RwLock},
    };

    use subprocess::{unix::PopenExt, Exec, ExitStatus};

    use crate::{
        errors::{TogetherInternalError, TogetherResult},
        log, log_err,
    };

    use super::ProcessId;

    pub struct SbProcess {
        popen: subprocess::Popen,
        mute: Option<Arc<RwLock<bool>>>,
    }

    impl SbProcess {
        pub fn spawn(command: &str, cwd: Option<&str>, raw: bool) -> TogetherResult<Self> {
            let process = Exec::shell(command)
                .stdout(if raw {
                    subprocess::Redirection::None
                } else {
                    subprocess::Redirection::Pipe
                })
                .stderr(if raw {
                    subprocess::Redirection::None
                } else {
                    subprocess::Redirection::Pipe
                });

            let process = if let Some(cwd) = cwd {
                process.cwd(cwd)
            } else {
                process
            };

            let popen = process.popen()?;
            let mute = Arc::new(RwLock::new(false));

            Ok(Self {
                popen,
                mute: Some(mute),
            })
        }

        pub fn mute(&self) {
            // TODO: remove
        }

        pub fn unmute(&self) {
            // TODO: remove
        }

        pub fn kill(&mut self) -> TogetherResult<()> {
            #[cfg(windows)]
            {
                Ok(self.popen.terminate()?)
            }
            #[cfg(unix)]
            {
                Ok(self.popen.send_signal(libc::SIGHUP)?)
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
}
