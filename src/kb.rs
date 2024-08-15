use crate::{
    config::{self, StartTogetherOptions},
    errors::TogetherResult,
    log, log_err,
    manager::{self, ProcessAction},
    t_println,
    terminal::Terminal,
    terminal_ext::TerminalExt,
};

#[derive(Default)]
struct InputState {
    requested_quit: bool,
    awaiting_quit_command: bool,
    last_command: Option<String>,
}

pub fn block_for_user_input(
    start_opts: &StartTogetherOptions,
    sender: manager::ProcessManagerHandle,
) -> TogetherResult<()> {
    use std::io::Write;
    use termion::event::Key;
    use termion::input::TermRead;

    let mut state = InputState::default();

    // let mut stdout = std::io::stdout().into_raw_mode().unwrap();
    let mut stdout = std::io::stdout();
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
            Key::Char('h') | Key::Char('?') => {
                log!("[help]");
                t_println!("together is a tool to run multiple commands in parallel selectively by an interactive prompt.");

                t_println!();
                t_println!("Press 't' to trigger a one-time run");
                t_println!("Press '.' to re-trigger the last one-time run");
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
                    break;
                }

                log!("Press 'q' again to quit together");
                state.requested_quit = true;
                break;
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
                let command = Terminal::select_single_command(
                    "Pick command to run, or press 'q' to cancel",
                    &sender,
                    &start_opts.config.start_options.commands,
                )?;
                if let Some(command) = command {
                    sender.send(ProcessAction::Create(command.to_string()))?;
                    state.last_command = Some(command.to_string());
                }
            }
            Key::Char('.') => {
                if let Some(command) = &state.last_command {
                    sender.send(ProcessAction::Create(command.clone()))?;
                } else {
                    log!("No last command to re-trigger");
                }
            }
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
                        sender.send(ProcessAction::Kill(command.clone()))?;
                    }
                    for command in recipe_commands {
                        sender.send(ProcessAction::Create(command.clone()))?;
                    }
                }
            }
            Key::Char('\n') => {}
            Key::Char(c) => {
                log_err!("Unknown command: '{}'", c);
                log!("Press 'h' or '?' for help");
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