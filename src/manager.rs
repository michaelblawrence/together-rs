use std::{collections::HashMap, sync::mpsc};

use crate::{
    errors::{TogetherError, TogetherResult},
    log, log_err,
    process::{Process, ProcessId},
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

pub struct ProcessManager {
    processes: HashMap<ProcessId, Process>,
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

    fn rx_message_loop(mut self) {
        loop {
            match self.receiver.try_recv() {
                Ok(message) => {
                    let response = self.process_message(message.0);
                    message.1.send(response).unwrap();
                }
                Err(mpsc::TryRecvError::Empty) => {
                    std::thread::sleep(std::time::Duration::from_millis(100));

                    if !self.processes.is_empty() {
                        self.cleanup_dead_processes();

                        if self.processes.is_empty() {
                            if self.quit_on_completion {
                                log!("All processes have exited, stopping...");
                                std::process::exit(0);
                            }

                            log!("No more processes running, waiting for new commands...");
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    break;
                }
            }
        }
    }

    fn process_message(&mut self, payload: ProcessAction) -> ProcessActionResponse {
        match payload {
            ProcessAction::Create(command) => {
                let id = self.index;
                self.index += 1;

                match Process::spawn(&command, self.raw_stdio) {
                    Ok(mut child) => {
                        let id = ProcessId::new(id, command);
                        if !self.raw_stdio {
                            child.forward_stdio(&id);
                        }
                        self.processes.insert(id.clone(), child);
                        log!("Started {}", id);
                        ProcessActionResponse::Created
                    }
                    Err(e) => ProcessActionResponse::Error(ProcessManagerError::SpawnChildFailed(
                        e.to_string(),
                    )),
                }
            }
            ProcessAction::Kill(id) => match self.processes.get_mut(&id) {
                Some(child) => match child.kill() {
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
            ProcessAction::KillAll => {
                let mut errors = vec![];
                for (id, child) in self.processes.iter_mut() {
                    match child.kill() {
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
