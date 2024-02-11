use clap::Parser;
use manager::ProcessAction;

mod errors;
mod manager;
mod terminal;

fn main() {
    let opts = terminal::Opts::parse();
    match opts.sub {
        terminal::SubCommand::Run(run_opts) => {
            let result = run_command(run_opts);
            if let Err(e) = result {
                log_err!("Unexpected error: {}", e);
                std::process::exit(1);
            }
        }
    }
}

fn run_command(opts: terminal::Run) -> Result<(), errors::TogetherError> {
    let manager = manager::ProcessManager::new()
        .with_raw_mode(opts.raw)
        .with_exit_on_error(opts.exit_on_error)
        .start();

    let sender = manager.subscribe();

    if opts.all {
        for command in &opts.commands {
            sender.send(ProcessAction::Create(command.to_string()))?;
        }
    } else {
        let selected_commands =
            terminal::Terminal::select_multiple("Select commands to run together", &opts.commands);
        for command in selected_commands {
            sender.send(ProcessAction::Create(command.clone()))?;
        }
    }

    handle_user_input(opts, sender)?;

    std::mem::drop(manager);
    Ok(())
}

fn handle_user_input(
    opts: terminal::Run,
    sender: manager::ProcessManagerHandle,
) -> Result<(), errors::TogetherError> {
    let mut input = String::new();
    loop {
        std::io::stdin().read_line(&mut input)?;
        match input.trim() {
            "h" | "?" => {
                log!("[help]");
                println!("together is a tool to run multiple commands in parallel selectively by an interactive prompt.");

                println!("");
                println!("Press 't' to trigger a one-time run");
                println!("Press 'k' to kill a running command");
                println!("Press 'r' to restart a running command");
                println!("Press 'l' to list all running commands");
                println!("Press 'h' or '?' to show this help message");
                println!("Press 'q' to stop");
                println!("");

                println!("");
                log!("[status]");
                let response = sender.send(ProcessAction::List)?;
                match response {
                    manager::ProcessActionResponse::List(list) => {
                        println!("together is running {} commands in parallel:", list.len());
                        for command in list {
                            println!("{}", command);
                        }
                    }
                    _ => {
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
                let response = sender.send(ProcessAction::List)?;
                match response {
                    manager::ProcessActionResponse::List(list) => {
                        for command in list {
                            println!("{}", command);
                        }
                    }
                    _ => {
                        log_err!("Unknown response");
                    }
                }
            }
            "k" => {
                let response = sender.send(ProcessAction::List)?;
                match response {
                    manager::ProcessActionResponse::List(list) => {
                        let command = terminal::Terminal::select_single(
                            "Pick command to kill, or press 'q' to cancel",
                            &list,
                        );
                        sender.send(ProcessAction::Kill(command.clone()))?;
                    }
                    _ => {
                        log_err!("Unknown response");
                    }
                }
            }
            "r" => {
                let response = sender.send(ProcessAction::List)?;
                match response {
                    manager::ProcessActionResponse::List(list) => {
                        let command = terminal::Terminal::select_single(
                            "Pick command to restart, or press 'q' to cancel",
                            &list,
                        );
                        sender.send(ProcessAction::Kill(command.clone()))?;
                        sender.send(ProcessAction::Create(command.command().to_string()))?;
                    }
                    _ => {
                        log_err!("Unknown response");
                    }
                }
            }
            "t" => {
                let command = terminal::Terminal::select_single(
                    "Pick command to run, or press 'q' to cancel",
                    &opts.commands,
                );
                sender.send(ProcessAction::Create(command.clone()))?;
            }
            _ => {
                log_err!("Unknown command: {}", input);
            }
        }
        input.clear();
    }
    Ok(())
}
