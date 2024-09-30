use std::ops::ControlFlow;

use crate::{
    config::{self, StartTogetherOptions},
    errors::TogetherResult,
    log, log_err,
    manager::{self, ProcessAction},
    process, t_println,
    terminal::Terminal,
    terminal_ext::TerminalExt,
};

#[derive(Default)]
struct InputState {
    requested_quit: bool,
    awaiting_quit_command: bool,
    last_command: Option<BufferedCommand>,
}

enum BufferedCommand {
    Start(String),
    Restart(String, process::ProcessId),
}

enum Key {
    #[cfg(feature = "termion")]
    CtrlC,
    Char(char),
}

#[cfg(feature = "termion")]
impl TryFrom<termion::event::Key> for Key {
    type Error = ();

    fn try_from(key: termion::event::Key) -> Result<Self, Self::Error> {
        match key {
            termion::event::Key::Ctrl('c') => Ok(Self::CtrlC),
            termion::event::Key::Char(c) => Ok(Self::Char(c)),
            _ => Err(()),
        }
    }
}

impl From<char> for Key {
    fn from(c: char) -> Self {
        Self::Char(c)
    }
}

#[cfg(feature = "termion")]
pub fn block_for_user_input(
    start_opts: &StartTogetherOptions,
    sender: manager::ProcessManagerHandle,
) -> TogetherResult<()> {
    use std::io::Write;
    // use termion::event::Key;
    use termion::input::TermRead;

    let mut state = InputState::default();

    // let mut stdout = std::io::stdout().into_raw_mode().unwrap();
    let mut stdout = std::io::stdout();
    let stdin = std::io::stdin();

    for k in stdin.keys() {
        let Ok(k): Result<Key, ()> = k?.try_into() else {
            continue;
        };

        match handle_key_press(k, &mut state, start_opts, &sender) {
            Ok(ControlFlow::Continue(_)) => {
                write!(stdout, "{}", termion::cursor::Show).unwrap();
                stdout.flush().unwrap();
            }
            Ok(ControlFlow::Break(_)) => break,
            Err(e) => {
                log_err!("Unexpected error: {:?}", e);
            }
        }
    }

    drop(stdout);
    Ok(())
}

#[cfg(not(feature = "termion"))]
pub fn block_for_user_input(
    start_opts: &StartTogetherOptions,
    sender: manager::ProcessManagerHandle,
) -> TogetherResult<()> {
    let mut state = InputState::default();
    let mut input = String::new();
    loop {
        std::io::stdin().read_line(&mut input)?;
        let Some(key) = input.trim().chars().next() else {
            continue;
        };

        match handle_key_press(key.into(), &mut state, start_opts, &sender) {
            Ok(ControlFlow::Continue(_)) => {}
            Ok(ControlFlow::Break(_)) => break,
            Err(e) => {
                log_err!("Unexpected error: {:?}", e);
            }
        }

        input.clear();
    }
    Ok(())
}

fn handle_key_press(
    key: Key,
    state: &mut InputState,
    start_opts: &StartTogetherOptions,
    sender: &manager::ProcessManagerHandle,
) -> TogetherResult<ControlFlow<()>> {
    if state.requested_quit {
        state.requested_quit = false;
        state.awaiting_quit_command = true;
    }

    match key {
        #[cfg(feature = "termion")]
        Key::CtrlC => {
            log!("Ctrl-C pressed, stopping all processes...");
            sender
                .send(ProcessAction::KillAll)
                .expect("Could not send signal on channel.");
        }
        Key::Char('h') | Key::Char('?') => {
            log!("[help]");
            t_println!("together is a tool to run multiple commands in parallel selectively by an interactive prompt.");

            t_println!();
            t_println!("Press 't' to trigger a one-time run");
            t_println!("Press '.' to re-trigger the last one-time run or restart action");
            if let Some(last) = &state.last_command {
                t_println!(
                    "  (last command: [{}] {})",
                    match last {
                        BufferedCommand::Start(_) => "start",
                        BufferedCommand::Restart(_, _) => "restart",
                    },
                    match last {
                        BufferedCommand::Start(command) => command,
                        BufferedCommand::Restart(command, _) => command,
                    }
                );
            }
            t_println!("Press 'b' to batch trigger commands by recipe");
            t_println!("Press 'z' to switch to running a single recipe");
            t_println!("Press 'k' to kill a running command");
            t_println!("Press 'r' to restart a running command");
            t_println!("Press 'l' to list all running commands");
            t_println!("Press 'd' to dump the current configuration");
            t_println!("Press 'h' or '?' to show this help message");
            t_println!("Press 'q' to stop");
            t_println!();

            t_println!();
            log!("[status]");
            match sender.list() {
                Ok(list) => {
                    t_println!("together is running {} commands in parallel:", list.len());
                    for command in list {
                        t_println!("  {}", command);
                    }
                }
                Err(_) => {
                    t_println!("together is running in an unknown state");
                }
            }
        }
        Key::Char('q') => {
            if state.awaiting_quit_command {
                log!("Quitting together...");
                sender.send(ProcessAction::KillAll)?;
                return Ok(ControlFlow::Break(()));
            }

            log!("Press 'q' again to quit together");
            state.requested_quit = true;
            return Ok(ControlFlow::Break(()));
        }
        Key::Char('l') => {
            for command in sender.list()? {
                t_println!("{}", command);
            }
        }
        Key::Char('d') => {
            let list = sender.list()?;
            let running: Vec<_> = list.iter().map(|c| c.command()).collect();
            let config = start_opts.config.clone();
            let config = config.with_running(&running);
            config::dump(&config)?;
        }
        Key::Char('k') => {
            let list = sender.list()?;
            let command = Terminal::select_single_process(
                "Pick command to kill, or press 'q' to cancel",
                &sender,
                &list,
            )?;
            if let Some(command) = command {
                sender.kill(command.clone())?;
            }
        }
        Key::Char('K') => {
            let list = sender.list()?;
            let command = Terminal::select_single_process(
                "Pick command to kill, or press 'q' to cancel",
                &sender,
                &list,
            )?;
            let signal = Terminal::select_single(
                "Pick signal to send, or press 'q' to cancel",
                &["SIGINT", "SIGTERM", "SIGKILL"],
            );
            let target = signal
                .and_then(|signal| match *signal {
                    "SIGINT" => Some(process::ProcessSignal::SIGINT),
                    "SIGTERM" => Some(process::ProcessSignal::SIGTERM),
                    "SIGKILL" => Some(process::ProcessSignal::SIGKILL),
                    _ => None,
                })
                .and_then(|signal| command.map(|command| (command, signal)));
            if let Some((command, signal)) = target {
                sender.send(ProcessAction::KillAdvanced(command.clone(), signal))?;
            }
        }
        Key::Char('r') => {
            let list = sender.list()?;
            let command = Terminal::select_single_process(
                "Pick command to restart, or press 'q' to cancel",
                &sender,
                &list,
            )?;
            if let Some(command) = command {
                sender.send(ProcessAction::Kill(command.clone()))?;
                let process_id = sender.spawn(command.command())?;
                state.last_command = Some(BufferedCommand::Restart(
                    command.command().to_string(),
                    process_id,
                ));
            }
        }
        Key::Char('t') => {
            let command = Terminal::select_single_command(
                "Pick command to run, or press 'q' to cancel",
                &sender,
                &start_opts.config.start_options.commands,
            )?;
            if let Some(command) = command {
                sender.spawn(command)?;
                state.last_command = Some(BufferedCommand::Start(command.to_string()));
            }
        }
        Key::Char('.') => match &state.last_command {
            Some(BufferedCommand::Start(command)) => {
                sender.spawn(command)?;
            }
            Some(BufferedCommand::Restart(command, process_id)) => {
                match sender.restart(process_id.clone(), &command)? {
                    Some(id) => {
                        let command = command.clone();
                        state.last_command = Some(BufferedCommand::Restart(command, id))
                    }
                    None => {
                        log_err!("Could not find process to restart");
                    }
                };
            }
            _ => {
                log!("No last command to re-trigger");
            }
        },
        Key::Char('b') => {
            let all_recipes = config::get_unique_recipes(&start_opts.config.start_options);
            let all_recipes = all_recipes.into_iter().cloned().collect::<Vec<_>>();
            let recipes = Terminal::select_multiple_recipes(
                "Select one or more recipes to start running, or press 'q' to cancel",
                &sender,
                &all_recipes,
            )?;
            let commands =
                config::collect_commands_by_recipes(&start_opts.config.start_options, &recipes);
            for command in commands {
                sender.send(ProcessAction::Create(command.clone()))?;
            }
        }
        Key::Char('z') => {
            let all_recipes = config::get_unique_recipes(&start_opts.config.start_options);
            let all_recipes = all_recipes.into_iter().cloned().collect::<Vec<_>>();
            let recipe = Terminal::select_single_recipe(
                "Select a recipe to start running, or press 'q' to cancel (note: this will stop all other commands)",
                &sender,
                &all_recipes,
            )?;
            if let Some(recipe) = recipe {
                let recipe = recipe.clone();
                let recipe_commands = config::collect_commands_by_recipes(
                    &start_opts.config.start_options,
                    &[recipe],
                );
                let list = sender.list()?;
                let kill_commands: Vec<_> = list
                    .iter()
                    .filter(|c| !recipe_commands.contains(&c.command().to_string()))
                    .collect();

                for command in kill_commands {
                    sender.kill(command.clone())?;
                }
                for command in recipe_commands {
                    sender.spawn(&command)?;
                }
            }
        }
        Key::Char('\n') => {}
        Key::Char(c) => {
            log_err!("Unknown command: '{}'", c);
            log!("Press 'h' or '?' for help");
        }
    }
    state.awaiting_quit_command = false;

    Ok(ControlFlow::Continue(()))
}
