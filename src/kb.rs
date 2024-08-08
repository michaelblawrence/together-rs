use crate::{
    config::{self, StartTogetherOptions},
    errors::TogetherResult,
    log, log_err,
    manager::{self, ProcessAction},
    terminal::Terminal,
    terminal_ext::TerminalExt,
};

#[derive(Default)]
struct InputState {
    requested_quit: bool,
    awaiting_quit_command: bool,
}

pub fn block_for_user_input(
    start_opts: &StartTogetherOptions,
    sender: manager::ProcessManagerHandle,
) -> TogetherResult<()> {
    use std::io::Write;
    use termion::event::Key;
    use termion::input::TermRead;
    use termion::raw::IntoRawMode;

    let mut state = InputState::default();

    let mut stdout = std::io::stdout().into_raw_mode().unwrap();
    let stdin = std::io::stdin();

    for k in stdin.keys() {
        if state.requested_quit {
            state.requested_quit = false;
            state.awaiting_quit_command = true;
        }
        match k? {
            Key::Ctrl('c') => {
                log!("Ctrl-C pressed, stopping all processes...");
                sender
                    .send(ProcessAction::KillAll)
                    .expect("Could not send signal on channel.");
            }
            // Key::Char('a') => {
            //     println!("Hello, world!");
            // }
            // Key::Char('b') => {
            //     println!("Hello, world!");
            //     println!("Hello, world!");
            //     println!("Hello, world!");
            // }
            // Key::Char('c') => {
            //     log!("Hello, world!");
            // }
            // Key::Char('d') => {
            //     log!("Hello, world!");
            //     log!("Hello, world!");
            //     log!("Hello, world!");
            // }
            Key::Char('h') | Key::Char('?') => {
                log!("[help]");
                println!("together is a tool to run multiple commands in parallel selectively by an interactive prompt.");

                println!();
                println!("Press 't' to trigger a one-time run");
                println!("Press 'b' to batch trigger a one-time run");
                println!("Press 'k' to kill a running command");
                println!("Press 'r' to restart a running command");
                println!("Press 'l' to list all running commands");
                println!("Press 'd' to dump the current configuration");
                println!("Press 'h' or '?' to show this help message");
                println!("Press 'q' to stop");
                println!();

                println!();
                log!("[status]");
                match sender.list() {
                    Ok(list) => {
                        println!("together is running {} commands in parallel:", list.len());
                        for command in list {
                            println!("  {}", command);
                        }
                    }
                    Err(_) => {
                        println!("together is running in an unknown state");
                    }
                }
            }
            Key::Char('q') => {
                if state.awaiting_quit_command {
                    log!("Quitting together...");
                    sender.send(ProcessAction::KillAll)?;
                    break;
                }

                log!("Press 'q' again to quit together");
                state.requested_quit = true;
                break;
            }
            Key::Char('l') => {
                for command in sender.list()? {
                    println!("{}", command);
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
                    sender.send(ProcessAction::Kill(command.clone()))?;
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
                    sender.send(ProcessAction::Create(command.command().to_string()))?;
                }
            }
            Key::Char('t') => {
                let all_commands = start_opts.config.start_options.as_commands();
                let command = Terminal::select_single_command(
                    "Pick command to run, or press 'q' to cancel",
                    &sender,
                    &all_commands,
                )?;
                if let Some(command) = command {
                    sender.send(ProcessAction::Create(command.clone()))?;
                }
            }
            Key::Char('b') => {
                let all_commands = start_opts.config.start_options.as_commands();
                let commands = Terminal::select_multiple_commands(
                    "Pick commands to run, or press 'q' to cancel",
                    &sender,
                    &all_commands,
                )?;
                for command in commands {
                    sender.send(ProcessAction::Create(command.clone()))?;
                }
            }
            Key::Char(c) => {
                log_err!("Unknown command: {}", c);
            }
            _ => {}
        }
        state.awaiting_quit_command = false;
        write!(stdout, "{}", termion::cursor::Show).unwrap();
        stdout.flush().unwrap();
    }

    drop(stdout);
    Ok(())
}
