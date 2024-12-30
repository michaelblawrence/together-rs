use std::sync::{Arc, Mutex};

use config::StartTogetherOptions;
use errors::TogetherResult;
use manager::ProcessAction;
use terminal_ext::TerminalExt;

pub mod config;
pub mod errors;
pub mod kb;
pub mod manager;
pub mod process;
pub mod terminal;
pub mod terminal_ext;

pub fn start(options: StartTogetherOptions) -> TogetherResult<()> {
    let StartTogetherOptions {
        config,
        working_directory,
        ..
    } = &options;

    let manager = manager::ProcessManager::new()
        .with_raw_mode(config.start_options.raw)
        .with_exit_on_error(config.start_options.exit_on_error)
        .with_quit_on_completion(config.start_options.quit_on_completion)
        .with_working_directory(working_directory.to_owned())
        .start();

    let sender = manager.subscribe();
    handle_ctrl_signal(sender);

    let selected_commands = collect_together_commands(&manager, &options)?;

    if config.start_options.no_init {
        log!("Skipping startup commands...");
    } else {
        execute_startup_commands(&manager, &config)?;
    }

    if config.start_options.init_only {
        log!("Finished running startup commands, waiting for user input... (press '?' for help)");
    } else {
        execute_together_commands(&manager, selected_commands)?;
    }

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
    options: &StartTogetherOptions,
) -> TogetherResult<Vec<String>> {
    if let Some(recipes) = &options.active_recipes {
        log!("Running commands from recipes...");
        let config_opts = &options.config.start_options;

        let selected_commands = config::collect_commands_by_recipes(&config_opts, recipes);

        log!("Commands selected by recipes:");
        for command in &selected_commands {
            log!("  - {}", command);
        }

        return Ok(selected_commands);
    }

    let config = &options.config;
    let selected_commands = match &config.running_commands() {
        Some(commands) => {
            log!("Running commands from configuration...");
            commands.into_iter().map(|c| c.to_string()).collect()
        }
        None if config.start_options.all => {
            log!("Running all commands...");
            config.start_options.as_commands()
        }
        None => {
            let all_commands = config.start_options.as_commands();
            let sender = manager.subscribe();
            let commands = terminal::Terminal::select_multiple_commands(
                "Select commands to run together",
                &sender,
                &all_commands,
            )?;
            commands.into_iter().cloned().collect()
        }
    };
    Ok(selected_commands)
}

fn execute_startup_commands(
    manager: &manager::ProcessManagerHandle,
    config: &config::TogetherConfigFile,
) -> TogetherResult<()> {
    let Some(startup) = &config.startup else {
        return Ok(());
    };

    log!("Running startup commands...");
    let sender = manager.subscribe();

    let commands = startup
        .iter()
        .flat_map(|index| index.retrieve(&config.start_options.commands))
        .map(|c| c.as_str().to_string())
        .collect::<Vec<_>>();

    let opts = if config.start_options.quiet_startup {
        manager::CreateOptions::default().with_stderr_only()
    } else {
        manager::CreateOptions::default()
    };

    for command in commands {
        let id = sender.spawn_advanced(&command, &opts)?;
        sender.wait(id)?;
        log!("Startup command '{}' completed", command);
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
