use std::{collections::HashMap, sync::mpsc};

use crate::{
    errors::{TogetherError, TogetherInternalError, TogetherResult},
    log, log_err,
    process::{Process, ProcessId, ProcessSignal, ProcessStdio},
};

pub enum ProcessAction {
    Create(String),
    CreateAdvanced(String, CreateOptions),
    Wait(ProcessId),
    Kill(ProcessId),
    KillAdvanced(ProcessId, ProcessSignal),
    KillAll,
    List,
}

#[derive(Debug)]
pub enum ProcessActionResponse {
    Created(ProcessId),
    Waited(mpsc::Receiver<()>),
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

#[derive(Default, Clone)]
pub struct CreateOptions {
    pub stdio: Option<ProcessStdio>,
    pub cwd: Option<String>,
}

impl CreateOptions {
    pub fn with_stderr_only(mut self) -> Self {
        self.stdio = Some(ProcessStdio::StderrOnly);
        self
    }
}

pub struct Message(ProcessAction, mpsc::Sender<ProcessActionResponse>);

pub struct ProcessManager {
    processes: HashMap<ProcessId, Process>,
    receiver: mpsc::Receiver<Message>,
    sender: mpsc::Sender<Message>,
    wait_handles: HashMap<ProcessId, mpsc::Sender<()>>,
    index: u32,
    raw_stdio: bool,
    exit_on_error: bool,
    quit_on_completion: bool,
    killed: bool,
    cwd: Option<String>,
}

impl ProcessManager {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            processes: HashMap::new(),
            receiver,
            sender,
            wait_handles: HashMap::new(),
            index: 0,
            raw_stdio: false,
            exit_on_error: false,
            quit_on_completion: true,
            killed: false,
            cwd: None,
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

    pub fn with_working_directory(mut self, working_directory: Option<String>) -> Self {
        self.cwd = working_directory;
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

    fn rx_message_loop(mut self) {
        let timeout = std::time::Duration::from_millis(100);
        loop {
            match self.receiver.recv_timeout(timeout) {
                Ok(message) => {
                    let response = self.process_message(message.0);
                    message.1.send(response).unwrap();
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if self.killed {
                        break;
                    }
                    if !self.processes.is_empty() {
                        self.cleanup_dead_processes();

                        if self.processes.is_empty() {
                            if self.quit_on_completion || self.killed {
                                log!("All processes have exited, stopping...");
                                std::process::exit(0);
                            }

                            match self
                                .receiver
                                .recv_timeout(std::time::Duration::from_millis(100))
                            {
                                Ok(Message(ProcessAction::KillAll, _)) => {
                                    std::process::exit(0);
                                }
                                Ok(message) => {
                                    let response = self.process_message(message.0);
                                    message.1.send(response).unwrap();
                                }
                                Err(mpsc::RecvTimeoutError::Timeout) => {
                                    log!("No more processes running, waiting for new commands...");
                                }
                                Err(mpsc::RecvTimeoutError::Disconnected) => {
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }

        std::process::exit(0);
    }

    fn process_message(&mut self, payload: ProcessAction) -> ProcessActionResponse {
        match payload {
            ProcessAction::Create(command) => {
                let id = self.index;
                self.index += 1;

                self.start_new_process(command, self.cwd.clone(), self.raw_stdio.into(), id)
            }
            ProcessAction::CreateAdvanced(command, options) => {
                let id = self.index;
                self.index += 1;

                let raw = options.stdio.unwrap_or(self.raw_stdio.into());
                let cwd = options.cwd.clone().or_else(|| self.cwd.clone());

                self.start_new_process(command, cwd, raw, id)
            }
            ProcessAction::Wait(id) => match self.processes.get(&id) {
                Some(_) => {
                    let (sender, receiver) = mpsc::channel();
                    self.wait_handles.insert(id.clone(), sender);
                    ProcessActionResponse::Waited(receiver)
                }
                None => ProcessActionResponse::Error(ProcessManagerError::NoSuchProcess),
            },
            ProcessAction::Kill(id) => match self.processes.get_mut(&id) {
                Some(child) => match child.kill(None) {
                    Ok(_) => {
                        log!("Killing {}", id);
                        ProcessActionResponse::Killed
                    }
                    Err(e) => ProcessActionResponse::Error(ProcessManagerError::KillChildFailed(
                        e.to_string(),
                    )),
                },
                None => ProcessActionResponse::Error(ProcessManagerError::NoSuchProcess),
            },
            ProcessAction::KillAdvanced(id, signal) => match self.processes.get_mut(&id) {
                Some(child) => match child.kill(Some(&signal)) {
                    Ok(_) => {
                        log!("Killing {} with signal {:?}", id, signal);
                        ProcessActionResponse::Killed
                    }
                    Err(e) => ProcessActionResponse::Error(ProcessManagerError::KillChildFailed(
                        e.to_string(),
                    )),
                },
                None => ProcessActionResponse::Error(ProcessManagerError::NoSuchProcess),
            },
            ProcessAction::KillAll => {
                self.killed = true;

                let mut errors = vec![];
                for (id, child) in self.processes.iter_mut() {
                    match child.kill(None) {
                        Ok(_) => {
                            log!("Killing {}", id);
                        }
                        Err(e) => {
                            errors.push(ProcessManagerError::KillChildFailed(e.to_string()));
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
        }
    }

    fn start_new_process(
        &mut self,
        command: String,
        cwd: Option<String>,
        stdio: ProcessStdio,
        id: u32,
    ) -> ProcessActionResponse {
        match Process::spawn(&command, cwd.as_deref(), stdio) {
            Ok(mut child) => {
                let id = ProcessId::new(id, command);
                if let ProcessStdio::Inherit = stdio {
                    child.forward_stdio(&id);
                }
                self.processes.insert(id.clone(), child);
                log!("Started  {}", id);
                ProcessActionResponse::Created(id)
            }
            Err(e) => {
                ProcessActionResponse::Error(ProcessManagerError::SpawnChildFailed(e.to_string()))
            }
        }
    }

    fn cleanup_dead_processes(&mut self) {
        let mut remove = vec![];
        let mut kill_all = false;

        for (id, child) in self.processes.iter_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    remove.push(id.clone());
                    if status != 0 && self.exit_on_error {
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

        for id in remove {
            if let Some(handle) = self.wait_handles.remove(&id) {
                handle.send(()).unwrap();
            }
            self.processes.remove(&id);
            log!("Finished {}", id);
        }
        if kill_all {
            for (id, mut child) in self.processes.drain() {
                match child.kill(None) {
                    Ok(_) => {}
                    Err(e) => {
                        log_err!("Failed to kill {id} => {}", e);
                    }
                }
            }
        }
    }
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
    pub fn list(&self) -> TogetherResult<Vec<ProcessId>> {
        self.send(ProcessAction::List).and_then(|r| match r {
            ProcessActionResponse::List(list) => Ok(list),
            _ => Err(TogetherInternalError::UnexpectedResponse.into()),
        })
    }
    pub fn spawn(&self, command: &str) -> TogetherResult<ProcessId> {
        self.send(ProcessAction::Create(command.to_string()))
            .and_then(|r| match r {
                ProcessActionResponse::Created(id) => Ok(id),
                _ => Err(TogetherInternalError::UnexpectedResponse.into()),
            })
    }
    pub fn spawn_advanced(
        &self,
        command: &str,
        options: &CreateOptions,
    ) -> TogetherResult<ProcessId> {
        self.send(ProcessAction::CreateAdvanced(
            command.to_string(),
            options.clone(),
        ))
        .and_then(|r| match r {
            ProcessActionResponse::Created(id) => Ok(id),
            _ => Err(TogetherInternalError::UnexpectedResponse.into()),
        })
    }
    pub fn kill(&self, id: ProcessId) -> TogetherResult<Option<()>> {
        self.send(ProcessAction::Kill(id)).and_then(|r| match r {
            ProcessActionResponse::Killed => Ok(Some(())),
            ProcessActionResponse::Error(ProcessManagerError::NoSuchProcess) => Ok(None),
            _ => Err(TogetherInternalError::UnexpectedResponse.into()),
        })
    }
    pub fn restart(&self, id: ProcessId, command: &str) -> TogetherResult<Option<ProcessId>> {
        match self.kill(id)? {
            Some(()) => Ok(Some(self.spawn(command)?)),
            None => Ok(None),
        }
    }
    pub fn wait(&self, id: ProcessId) -> TogetherResult<()> {
        self.send(ProcessAction::Wait(id)).and_then(|r| match r {
            ProcessActionResponse::Waited(done) => done.recv().map_err(|e| e.into()),
            _ => Err(TogetherInternalError::UnexpectedResponse.into()),
        })
    }
}

impl Drop for ProcessManagerHandle {
    fn drop(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        let (sender, receiver) = mpsc::channel();

        if let Err(_) = self.sender.send(Message(ProcessAction::KillAll, sender)) {
            // the process manager has already exited, nothing to do
            return;
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
            Err(std::sync::mpsc::RecvError) => {
                // the process manager has already exited, nothing to do
            }
        }
    }
}
