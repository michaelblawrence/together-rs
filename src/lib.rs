use std::sync::{Arc, Mutex};

use config::StartTogetherOptions;
use errors::TogetherResult;
use manager::ProcessAction;
use terminal_ext::TerminalExt;

use crate::manager::ProcessActionResponse;

pub mod config;
pub mod errors;
pub mod kb;
pub mod manager;
pub mod process;
pub mod terminal;
pub mod terminal_ext;

pub fn start(options: StartTogetherOptions) -> TogetherResult<()> {
    let StartTogetherOptions {
        arg_command,
        override_commands,
        startup_commands,
        working_directory,
        config_path,
    } = &options;

    let manager = manager::ProcessManager::new()
        .with_raw_mode(arg_command.raw)
        .with_exit_on_error(arg_command.exit_on_error)
        .with_quit_on_completion(arg_command.quit_on_completion)
        .with_working_directory(working_directory.to_owned())
        .start();

    let sender = manager.subscribe();
    handle_ctrl_signal(sender);

    let selected_commands = collect_together_commands(&manager, override_commands, arg_command)?;

    execute_startup_commands(startup_commands, &manager)?;

    if arg_command.init_only {
        log!("Finished running startup commands, exiting...");
        return Ok(());
    }

    execute_together_commands(&manager, selected_commands)?;

    let sender = manager.subscribe();
    kb::block_for_user_input(&options, sender)?;

    std::mem::drop(manager);
    Ok(())
}

pub fn handle_ctrl_signal(sender: manager::ProcessManagerHandle) {
    let state = Arc::new(Mutex::new(false));
    let handler = ctrlc::set_handler(move || {
        {
            let mut state = state.lock().unwrap();
            if *state {
                log!("Ctrl-C pressed again, exiting immediately...");
                std::process::exit(1);
            }
            *state = true;
        }

        log!("Ctrl-C pressed, stopping all processes...");
        sender
            .send(ProcessAction::KillAll)
            .expect("Could not send signal on channel.");
    });
    handler.expect("Error setting Ctrl-C handler");
}

fn collect_together_commands(
    manager: &manager::ProcessManagerHandle,
    override_commands: &Option<Vec<String>>,
    arg_command: &terminal::RunCommand,
) -> TogetherResult<Vec<String>> {
    let sender = manager.subscribe();
    let selected_commands = match override_commands.as_ref() {
        Some(commands) => {
            log!("Running commands from configuration...");
            commands.iter().cloned().collect()
        }
        None if arg_command.all => {
            log!("Running all commands...");
            arg_command.commands.iter().cloned().collect()
        }
        None => {
            let commands = terminal::Terminal::select_multiple_commands(
                "Select commands to run together",
                &sender,
                &arg_command.commands,
            )?;
            commands.into_iter().cloned().collect()
        }
    };
    Ok(selected_commands)
}

fn execute_startup_commands(
    startup_commands: &Option<Vec<String>>,
    manager: &manager::ProcessManagerHandle,
) -> TogetherResult<()> {
    let Some(commands) = startup_commands else {
        return Ok(());
    };

    log!("Running startup commands...");
    let sender = manager.subscribe();

    for command in commands {
        match sender.send(ProcessAction::Create(command.clone()))? {
            ProcessActionResponse::Created(id) => match sender.send(ProcessAction::Wait(id))? {
                ProcessActionResponse::Waited(done) => {
                    done.recv()?;
                    log!("Startup command '{}' completed", command);
                }
                x => {
                    log_err!("Unexpected response from process manager: {:?}", x);
                }
            },
            x => {
                log_err!("Unexpected response from process manager: {:?}", x);
            }
        }
    }

    Ok(())
}

fn execute_together_commands(
    manager: &manager::ProcessManagerHandle,
    selected_commands: Vec<String>,
) -> TogetherResult<()> {
    let sender = manager.subscribe();
    for command in selected_commands {
        sender.send(ProcessAction::Create(command.clone()))?;
    }
    Ok(())
}
