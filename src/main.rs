use std::{
    io::{BufRead, Read},
    process::Stdio,
    sync::{Arc, Mutex},
};

use clap::Parser;
use dialoguer::{theme::ColorfulTheme, MultiSelect, Select};

// This is a tool similar to 'concurrently' and 'parallelshell' in Node.js,
// but for Rust. It allows you to run multiple commands in parallel selectively by an interactive prompt.

#[derive(Debug, clap::Parser)]
#[clap(name = "together", version = "1.0.0", author = "Michael L.")]
struct Opts {
    #[clap(subcommand)]
    run: RunSubCommand,
}

#[derive(Debug, clap::Parser)]
enum RunSubCommand {
    #[clap(name = "run", about = "Run commands in parallel")]
    Run(RunOpts),
}

#[derive(Debug, clap::Parser)]
struct RunOpts {
    #[clap(long = "it", default_value = "false", help = "Run interactively")]
    interactive: bool,

    #[clap(
        long = "raw",
        default_value = "false",
        help = "Enable raw input/output mode. e.g. 'ls -l' will no longer be printed as '[1] ls -l'"
    )]
    raw_io_mode: bool,

    #[clap(
        last = true,
        required = true,
        help = "Commands to run. e.g. 'ls -l', 'echo hello'"
    )]
    commands: Vec<String>,
}

fn main() {
    let opts: Opts = Opts::parse();
    match opts.run {
        RunSubCommand::Run(run_opts) => {
            run_command(run_opts);
        }
    }
}

fn run_command(run_opts: RunOpts) {
    let commands = prompt_commands(&run_opts, SelectionType::Together);

    let threads = Arc::new(Mutex::new(vec![]));
    let (spawn_sender, spawn_receiver) =
        std::sync::mpsc::channel::<(usize, String, std::process::Child)>();

    for (idx, (input, command)) in commands.into_iter().enumerate() {
        spawn_sender.send((idx, input, command)).unwrap();
    }

    let threads_clone = threads.clone();
    let spawn_thread = std::thread::spawn(move || {
        while let Ok((idx, input, mut command)) = spawn_receiver.recv() {
            let label = format!("[{}]", idx + 1);
            let (sender, receiver) = std::sync::mpsc::channel();
            let thread = if run_opts.raw_io_mode {
                let label_clone = label.clone();
                let input_clone = input.to_string();
                let thread = std::thread::spawn(move || loop {
                    if let Ok(()) = receiver.try_recv() {
                        match command.kill() {
                            Ok(_) => println!("[{} killed]: {}", &label_clone, input_clone),
                            Err(_) => {
                                println!("[{} failed to kill]: {}", &label_clone, input_clone);
                                break;
                            }
                        }
                    }
                    match command.try_wait() {
                        Ok(Some(_)) => {
                            break;
                        }
                        Ok(None) => {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                        Err(_) => {
                            break;
                        }
                    }
                });
                thread
            } else {
                let stdout = command.stdout.take().unwrap();
                let stderr = command.stderr.take().unwrap();

                let mut stdout = std::io::BufReader::new(stdout);
                let mut stderr = std::io::BufReader::new(stderr);
                let mut stdout_line = String::new();
                let mut stderr_line = String::new();

                let label_clone = label.clone();
                // in another thread, print stdout and stderr
                std::thread::spawn(move || loop {
                    if let Ok(()) = receiver.try_recv() {
                        command.kill().unwrap();
                    }
                    stdout_line.clear();
                    stderr_line.clear();
                    let stdout_read = stdout.read_line(&mut stdout_line);
                    let stderr_read = stderr.read_line(&mut stderr_line);
                    match (stdout_read, stderr_read) {
                        (Ok(0), Ok(0)) => {
                            break;
                        }
                        (Ok(_), Ok(_)) => {
                            print!("{} {}", label_clone, stdout_line);
                            eprint!("{} {}", label_clone, stderr_line);
                        }
                        (Ok(_), _) => {
                            print!("{} {}", label_clone, stdout_line);
                        }
                        (_, Ok(_)) => {
                            eprint!("{} {}", label_clone, stderr_line);
                        }
                        _ => {
                            break;
                        }
                    }
                })
            };
            let mut threads = threads_clone.lock().unwrap();
            threads.push((label.clone(), input, sender, Some(thread)));
        }
    });

    loop {
        // pause for a while
        std::thread::sleep(std::time::Duration::from_millis(100));

        let mut threads = threads.lock().unwrap();
        for (label, input, _, item) in threads.iter_mut() {
            if let Some(thread) = item {
                if thread.is_finished() {
                    item.take();
                    println!("[{} finished]: {}", label, input);
                }
            }
        }
        // remove finished threads
        threads.retain(|(_, _, _, item)| item.is_some());
        if threads.is_empty() {
            break;
        }

        // if stdin has input, check if it is "q", or "?", and then exit or print help
        let stdin = std::io::stdin();
        let mut stdin = stdin.lock();
        // read non-blocking
        let mut buf = [0; 1];
        if let Ok(_) = stdin.read(&mut buf) {
            let c = buf[0] as char;
            match c {
                'q' => {
                    println!("together is exiting...");
                    for (_, _, sender, _) in threads.iter() {
                        _ = sender.send(())
                    }
                }
                't' => {
                    println!("together is triggering a one-time run");
                    let opts_commands = get_chosen_commands(&run_opts, SelectionType::Once);
                    for opts_command in opts_commands {
                        let command = spawn(&opts_command, &run_opts);
                        println!("[x running]: {}", opts_command);
                        spawn_sender
                            .send((0, opts_command, command.unwrap()))
                            .unwrap();
                    }
                }
                'k' => {
                    println!("together is killing a running command");
                    let commands: Vec<String> = threads
                        .iter()
                        .map(|(label, input, _, _)| format!("{}: {}", label, input))
                        .collect();

                    match Select::with_theme(&ColorfulTheme::default())
                        .with_prompt("Pick command to kill, or press 'q' to cancel")
                        .items(&commands)
                        .interact_opt()
                    {
                        Ok(Some(index)) => {
                            let (_, _, sender, _) = &threads[index];
                            _ = sender.send(());
                            println!("Sent kill signal to {}", commands[index]);
                        }
                        Ok(None) => {
                            println!("No command selected");
                        }
                        Err(_) => {
                            println!("Error selecting command");
                        }
                    }
                }
                '?' => {
                    println!("together is running {} commands in parallel", threads.len());
                    for (label, input, _, _) in threads.iter() {
                        println!("{}: {}", label, input);
                    }
                    println!("Press 't' to trigger a one-time run");
                    println!("Press 'k' to kill a running command");
                    println!("Press 'q' to stop");
                }
                _ => {}
            }
        }
    }
}

fn prompt_commands(run_opts: &RunOpts, which: SelectionType) -> Vec<(String, std::process::Child)> {
    let opts_commands = get_chosen_commands(run_opts, which);
    // .into_iter()
    // // .map(|cmd| Box::leak(cmd.into_boxed_str()))
    // .collect::<Vec<_>>();
    let mut commands = vec![];

    for (idx, opts_command) in opts_commands.iter().enumerate() {
        let label = format!("[{}]", idx + 1);
        let command = spawn(opts_command, run_opts);
        println!("[{} running]: {}", label, opts_command);
        commands.push((opts_command.clone(), command.unwrap()));
    }
    commands
}

fn spawn(shell_command: &str, run_opts: &RunOpts) -> std::io::Result<std::process::Child> {
    let command = &mut std::process::Command::new("bash");
    let command = command.arg("-c").arg(shell_command).stdin(Stdio::null());

    let command = if run_opts.raw_io_mode {
        command.stdout(Stdio::inherit()).stderr(Stdio::inherit())
    } else {
        command.stdout(Stdio::piped()).stderr(Stdio::piped())
    };

    let command = command.spawn();
    command
}

enum SelectionType {
    Together,
    Once,
}

fn get_chosen_commands(run_opts: &RunOpts, which: SelectionType) -> Vec<String> {
    let opts_commands = match run_opts.interactive {
        true => {
            let mut opts_commands = vec![];
            let defaults = run_opts.commands.iter().map(|_| false).collect::<Vec<_>>();
            let multi_select = match which {
                SelectionType::Together => MultiSelect::with_theme(&ColorfulTheme::default())
                    .with_prompt("Pick commands to run together")
                    .items(&run_opts.commands[..])
                    .defaults(&defaults[..])
                    .interact(),
                SelectionType::Once => Select::with_theme(&ColorfulTheme::default())
                    .with_prompt("Pick command to run once")
                    .items(&run_opts.commands[..])
                    .interact()
                    .map(|index| vec![index]),
            };
            let selections = multi_select.unwrap();
            for index in selections {
                opts_commands.push(run_opts.commands[index].clone());
            }
            opts_commands
        }
        false => run_opts.commands.clone(),
    };
    if opts_commands.is_empty() {
        eprintln!("No commands to run");
        std::process::exit(0);
    }
    opts_commands
}
