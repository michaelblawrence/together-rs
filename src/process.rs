use std::{io::BufRead, sync::Arc};

use crate::{errors::TogetherResult, log_err};

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

pub struct Process(std::process::Child);

impl Process {
    pub fn spawn(command: &String, raw: bool) -> TogetherResult<Self> {
        let process = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(if raw {
                std::process::Stdio::inherit()
            } else {
                std::process::Stdio::piped()
            })
            .stderr(if raw {
                std::process::Stdio::inherit()
            } else {
                std::process::Stdio::piped()
            })
            .spawn()?;
        Ok(Self(process))
    }

    pub fn kill(&mut self) -> TogetherResult<()> {
        Ok(self.0.kill()?)
    }

    pub fn try_wait(&mut self) -> TogetherResult<Option<i32>> {
        Ok(self
            .0
            .try_wait()
            .map(|status| status.and_then(|s| s.code()))?)
    }

    pub fn forward_stdio(&mut self, id: &ProcessId) {
        let stdout = self.0.stdout.take().unwrap();
        let stderr = self.0.stderr.take().unwrap();
        let id = id.clone();
        std::thread::spawn(move || {
            let id = id.clone();
            Process::forward_stdio_blocking(&id, stdout, stderr)
        });
    }

    fn forward_stdio_blocking(
        id: &ProcessId,
        stdout: std::process::ChildStdout,
        stderr: std::process::ChildStderr,
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
