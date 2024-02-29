use std::sync::{Arc, Mutex};

use clap::Parser;
use config::RunContext;
use errors::TogetherResult;
use manager::ProcessAction;

use crate::manager::ProcessActionResponse;

mod config;
mod errors;
mod manager;
mod process;
mod terminal;

fn main() {
    let opts = terminal::Opts::parse();
    let context = config::to_run_context(opts);
    let result = run_command(context);
    if let Err(e) = result {
        log_err!("Unexpected error: {}", e);
        std::process::exit(1);
    }
}

fn run_command(context: RunContext) -> Result<(), errors::TogetherError> {
    let RunContext {
        opts,
        override_commands,
        startup_commands,
        working_directory,
    } = &context;

    let manager = manager::ProcessManager::new()
        .with_raw_mode(opts.raw)
        .with_exit_on_error(opts.exit_on_error)
        .with_quit_on_completion(opts.quit_on_completion)
        .with_working_directory(working_directory.to_owned())
        .start();

    let sender = manager.subscribe();
    handle_ctrl_signal(sender);

    let sender = manager.subscribe();

    let selected_commands = match override_commands.as_ref() {
        Some(commands) => {
            log!("Running commands from configuration...");
            commands.iter().collect()
        }
        None => {
            if opts.all {
                log!("Running all commands...");
                opts.commands.iter().collect()
            } else {
                let commands = select_multiple_commands(
                    "Select commands to run together",
                    &sender,
                    &opts.commands,
                )?;
                let config = config::Config {
                    run_opts: opts.clone(),
                    running: commands
                        .iter()
                        .map(|&c| opts.commands.iter().position(|x| x == c).unwrap())
                        .collect(),
                    startup: startup_commands.as_ref().map(|commands| {
                        commands
                            .iter()
                            .map(|c| opts.commands.iter().position(|x| x == c).unwrap())
                            .collect()
                    }),
                };
                config::save(&config)?;
                commands
            }
        }
    };

    if let Some(commands) = startup_commands {
        log!("Running startup commands...");
        for command in commands {
            match sender.send(ProcessAction::Create(command.clone()))? {
                ProcessActionResponse::Created(id) => {
                    match sender.send(ProcessAction::Wait(id))? {
                        ProcessActionResponse::Waited(done) => {
                            done.recv()?;
                            log!("Startup command '{}' completed", command);
                        }
                        _ => {
                            log_err!("Unexpected response from process manager");
                        }
                    }
                }
                _ => {
                    log_err!("Unexpected response from process manager");
                }
            }
        }
    }

    for command in selected_commands {
        sender.send(ProcessAction::Create(command.clone()))?;
    }

    block_for_user_input(&context, sender)?;

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

fn block_for_user_input(
    context: &RunContext,
    sender: manager::ProcessManagerHandle,
) -> Result<(), errors::TogetherError> {
    let mut input = String::new();
    loop {
        std::io::stdin().read_line(&mut input)?;
        match input.trim() {
            "h" | "?" => {
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
            "q" => {
                log!("Quitting...");
                sender.send(ProcessAction::KillAll)?;
                break;
            }
            "l" => {
                for command in sender.list()? {
                    println!("{}", command);
                }
            }
            "d" => {
                let list = sender.list()?;
                let running = list
                    .iter()
                    .map(|c| {
                        context
                            .opts
                            .commands
                            .iter()
                            .position(|x| x == c.command())
                            .unwrap()
                    })
                    .collect();

                let config = config::Config {
                    run_opts: context.opts.clone(),
                    running,
                    startup: context.startup_commands.as_ref().map(|commands| {
                        commands
                            .iter()
                            .map(|c| context.opts.commands.iter().position(|x| x == c).unwrap())
                            .collect()
                    }),
                };
                config::dump(&config)?;
            }
            "k" => {
                let list = sender.list()?;
                let command = select_single_process(
                    "Pick command to kill, or press 'q' to cancel",
                    &sender,
                    &list,
                )?;
                if let Some(command) = command {
                    sender.send(ProcessAction::Kill(command.clone()))?;
                }
            }
            "r" => {
                let list = sender.list()?;
                let command = select_single_process(
                    "Pick command to restart, or press 'q' to cancel",
                    &sender,
                    &list,
                )?;
                if let Some(command) = command {
                    sender.send(ProcessAction::Kill(command.clone()))?;
                    sender.send(ProcessAction::Create(command.command().to_string()))?;
                }
            }
            "t" => {
                let command = select_single_command(
                    "Pick command to run, or press 'q' to cancel",
                    &sender,
                    &context.opts.commands,
                )?;
                if let Some(command) = command {
                    sender.send(ProcessAction::Create(command.clone()))?;
                }
            }
            "b" => {
                let commands = select_multiple_commands(
                    "Pick commands to run, or press 'q' to cancel",
                    &sender,
                    &context.opts.commands,
                )?;
                for command in commands {
                    sender.send(ProcessAction::Create(command.clone()))?;
                }
            }
            "" => {}
            _ => {
                log_err!("Unknown command: {}", input);
            }
        }
        input.clear();
    }
    Ok(())
}

fn select_single_process<'a>(
    prompt: &'a str,
    sender: &'a manager::ProcessManagerHandle,
    list: &'a [process::ProcessId],
) -> TogetherResult<Option<&'a process::ProcessId>> {
    sender.send(ProcessAction::SetMute(true))?;
    let command = terminal::Terminal::select_single(prompt, list);
    sender.send(ProcessAction::SetMute(false))?;
    Ok(command)
}

fn select_single_command<'a>(
    prompt: &'a str,
    sender: &'a manager::ProcessManagerHandle,
    list: &'a [String],
) -> TogetherResult<Option<&'a String>> {
    sender.send(ProcessAction::SetMute(true))?;
    let command = terminal::Terminal::select_single(prompt, list);
    sender.send(ProcessAction::SetMute(false))?;
    Ok(command)
}

fn select_multiple_commands<'a>(
    prompt: &'a str,
    sender: &'a manager::ProcessManagerHandle,
    list: &'a [String],
) -> TogetherResult<Vec<&'a String>> {
    sender.send(ProcessAction::SetMute(true))?;
    let commands = terminal::Terminal::select_multiple(prompt, list);
    if commands.is_empty() {
        log!("No commands selected...");
    }
    sender.send(ProcessAction::SetMute(false))?;
    Ok(commands)
}
