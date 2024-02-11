use std::{
    collections::HashMap,
    io::BufRead,
    process::{ChildStderr, ChildStdout},
    sync::{mpsc, Arc},
};

use crate::{
    errors::{TogetherError, TogetherResult},
    log, log_err,
};

pub enum ProcessAction {
    Create(String),
    Kill(ProcessId),
    KillAll,
    List,
}

pub enum ProcessActionResponse {
    Created,
    Killed,
    KilledAll,
    List(Vec<ProcessId>),
    Error(ProcessManagerError),
}

#[derive(Debug)]
pub enum ProcessManagerError {
    SpawnChildFailed(String),
    KillChildFailed(String),
    NoSuchProcess,
    Unknown,
}

pub struct Message(ProcessAction, mpsc::Sender<ProcessActionResponse>);

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct ProcessId {
    id: u32,
    command: Arc<str>,
}

impl ProcessId {
    pub fn command(&self) -> &str {
        &self.command
    }
    pub fn id(&self) -> u32 {
        self.id
    }
}

impl std::fmt::Display for ProcessId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[{}]: {}", self.id, self.command)
    }
}

pub struct ProcessManager {
    processes: HashMap<ProcessId, std::process::Child>,
    receiver: mpsc::Receiver<Message>,
    sender: mpsc::Sender<Message>,
    index: u32,
    raw_stdio: bool,
    exit_on_error: bool,
    quit_on_completion: bool,
}

impl ProcessManager {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            processes: HashMap::new(),
            receiver,
            sender,
            index: 0,
            raw_stdio: false,
            exit_on_error: false,
            quit_on_completion: true,
        }
    }

    pub fn with_raw_mode(mut self, raw_mode: bool) -> Self {
        self.raw_stdio = raw_mode;
        self
    }

    pub fn with_exit_on_error(mut self, exit_on_error: bool) -> Self {
        self.exit_on_error = exit_on_error;
        self
    }

    pub fn with_quit_on_completion(mut self, quit_on_completion: bool) -> Self {
        self.quit_on_completion = quit_on_completion;
        self
    }

    pub fn start(self) -> ProcessManagerHandle {
        let sender = self.sender.clone();
        let thread = std::thread::spawn(move || self.rx_message_loop());
        ProcessManagerHandle {
            thread: Some(thread),
            sender,
        }
    }

    fn child_stdio_loop(id: &ProcessId, stdout: ChildStdout, stderr: ChildStderr) {
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

    fn rx_message_loop(mut self) {
        loop {
            match self.receiver.try_recv() {
                Ok(message) => {
                    let response = match message.0 {
                        ProcessAction::Create(command) => {
                            let id = self.index;
                            self.index += 1;

                            match spawn_process(&command, self.raw_stdio) {
                                Ok(mut child) => {
                                    let id = ProcessId {
                                        id,
                                        command: command.into_boxed_str().into(),
                                    };
                                    if !self.raw_stdio {
                                        spawn_stdio_thread(&id, &mut child);
                                    }
                                    self.processes.insert(id.clone(), child);
                                    log!("Started {}", id);
                                    ProcessActionResponse::Created
                                }
                                Err(e) => ProcessActionResponse::Error(
                                    ProcessManagerError::SpawnChildFailed(e.to_string()),
                                ),
                            }
                        }
                        ProcessAction::Kill(id) => match self.processes.get_mut(&id) {
                            Some(child) => match child.kill() {
                                Ok(_) => {
                                    log!("Killing {}", id);
                                    ProcessActionResponse::Killed
                                }
                                Err(e) => ProcessActionResponse::Error(
                                    ProcessManagerError::KillChildFailed(e.to_string()),
                                ),
                            },
                            None => {
                                ProcessActionResponse::Error(ProcessManagerError::NoSuchProcess)
                            }
                        },
                        ProcessAction::KillAll => {
                            let mut errors = vec![];
                            for (id, child) in self.processes.iter_mut() {
                                match child.kill() {
                                    Ok(_) => {
                                        log!("Killing {}", id);
                                    }
                                    Err(e) => {
                                        errors.push(ProcessManagerError::KillChildFailed(
                                            e.to_string(),
                                        ));
                                    }
                                }
                            }
                            if errors.is_empty() {
                                ProcessActionResponse::KilledAll
                            } else {
                                ProcessActionResponse::Error(ProcessManagerError::Unknown)
                            }
                        }
                        ProcessAction::List => {
                            let list = self.processes.keys().cloned().collect();
                            ProcessActionResponse::List(list)
                        }
                    };
                    message.1.send(response).unwrap();
                }
                Err(mpsc::TryRecvError::Empty) => {
                    std::thread::sleep(std::time::Duration::from_millis(100));

                    let mut remove = vec![];
                    let mut kill_all = false;

                    for (id, child) in self.processes.iter_mut() {
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                remove.push(id.clone());
                                if !status.success() && self.exit_on_error {
                                    log_err!("{}: exited with non-zero status", id);
                                    kill_all = true;
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                log_err!("Failed to check child status: {}", e);
                            }
                        }
                    }

                    let had_processes = !self.processes.is_empty();
                    for id in remove {
                        self.processes.remove(&id);
                        log!("Exited {}", id);
                    }
                    if kill_all {
                        for (id, mut child) in self.processes.drain() {
                            match child.kill() {
                                Ok(_) => {}
                                Err(e) => {
                                    log_err!("Failed to kill {id} => {}", e);
                                }
                            }
                        }
                    }
                    let have_processes = !self.processes.is_empty();

                    if had_processes && !have_processes {
                        log!("All processes have exited");
                        if self.quit_on_completion {
                            log!("Stopping...");
                            std::process::exit(0);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    break;
                }
            }
        }
    }
}

// TODO: use https://github.com/hniksic/rust-subprocess
fn spawn_stdio_thread(id: &ProcessId, child: &mut std::process::Child) {
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let id = id.clone();
    std::thread::spawn(move || {
        let id = id.clone();
        ProcessManager::child_stdio_loop(&id, stdout, stderr)
    });
}

fn spawn_process(command: &String, raw: bool) -> Result<std::process::Child, std::io::Error> {
    std::process::Command::new("sh")
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
        .spawn()
}

pub struct ProcessManagerHandle {
    thread: Option<std::thread::JoinHandle<()>>,
    sender: mpsc::Sender<Message>,
}

impl ProcessManagerHandle {
    pub fn send(&self, action: ProcessAction) -> TogetherResult<ProcessActionResponse> {
        let (sender, receiver) = mpsc::channel();
        self.sender
            .send(Message(action, sender))
            .map_err(|e| TogetherError::DynError(e.into()))?;
        receiver.recv().map_err(|e| e.into())
    }
    pub fn subscribe(&self) -> ProcessManagerHandle {
        ProcessManagerHandle {
            thread: None,
            sender: self.sender.clone(),
        }
    }
}

impl Drop for ProcessManagerHandle {
    fn drop(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        match self.sender.send(Message(ProcessAction::KillAll, sender)) {
            Ok(_) => {}
            Err(e) => {
                log_err!("Failed to send kill all message: {}", e);
                return;
            }
        };
        match receiver.recv() {
            Ok(ProcessActionResponse::KilledAll) => {
                if let Err(e) = thread.join() {
                    log_err!("Failed to join process manager thread: {:?}", e);
                }
            }
            Ok(ProcessActionResponse::Error(response)) => {
                log_err!("Failed to kill all processes: {:?}", response);
            }
            Ok(_) => {
                log_err!("Received unexpected kill all response");
            }
            Err(e) => {
                log_err!("Failed to receive kill all response: {}", e);
            }
        }
    }
}
