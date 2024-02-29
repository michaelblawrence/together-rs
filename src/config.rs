use crate::{errors::TogetherResult, log, log_err, terminal};

pub struct RunContext {
    pub opts: terminal::Run,
    pub override_commands: Option<Vec<String>>,
    pub startup_commands: Option<Vec<String>>,
    pub working_directory: Option<String>,
}

pub fn to_run_context(opts: terminal::Opts) -> RunContext {
    let (run_opts, selected_commands, config) = match opts.sub {
        terminal::SubCommand::Run(run_opts) => (run_opts, None, None),

        terminal::SubCommand::Rerun(_) => {
            if opts.no_config {
                log_err!("To use rerun, you must have a configuration file");
                std::process::exit(1);
            }
            match load() {
                Ok(config) => {
                    let commands = get_running_commands(&config, &config.running);
                    (
                        config.run_opts,
                        Some(commands).filter(|c| !c.is_empty()),
                        None,
                    )
                }
                Err(e) => {
                    log_err!("Failed to load configuration: {}", e);
                    std::process::exit(1);
                }
            }
        }

        terminal::SubCommand::Load(load) => {
            if opts.no_config {
                log_err!("To use rerun, you must have a configuration file");
                std::process::exit(1);
            }
            match load_from(load.path) {
                Ok(config) => {
                    let running = &config.running;
                    let commands = get_running_commands(&config, running);
                    (
                        config.run_opts.clone(),
                        Some(commands).filter(|c| !c.is_empty()),
                        Some(config),
                    )
                }
                Err(e) => {
                    log_err!("Failed to load configuration: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    RunContext {
        opts: run_opts,
        override_commands: selected_commands,
        startup_commands: config.and_then(|c| {
            c.startup.map(|s| {
                s.iter()
                    .filter_map(|&i| c.run_opts.commands.get(i).cloned())
                    .collect()
            })
        }),
        working_directory: opts.working_directory,
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub run_opts: crate::terminal::Run,
    pub running: Vec<usize>,
    pub startup: Option<Vec<usize>>,
}

pub fn load_from(config_path: impl AsRef<std::path::Path>) -> TogetherResult<Config> {
    let config = std::fs::read_to_string(config_path)?;
    let config: Config = toml::from_str(&config)?;
    Ok(config)
}

pub fn load() -> TogetherResult<Config> {
    let config_path = path();
    log!("Loading configuration from: {:?}", config_path);
    load_from(config_path)
}

pub fn save(config: &Config) -> TogetherResult<()> {
    let config_path = path();
    log!("Loading configuration from: {:?}", config_path);
    let config = toml::to_string_pretty(config)?;
    std::fs::write(config_path, config)?;
    Ok(())
}

pub fn dump(config: &Config) -> TogetherResult<()> {
    let config = toml::to_string(config)?;
    println!("Configuration:");
    println!();
    println!("{}", config);
    Ok(())
}

pub fn get_running_commands(config: &Config, running: &[usize]) -> Vec<String> {
    let commands: Vec<String> = running
        .iter()
        .filter_map(|index| config.run_opts.commands.get(*index).cloned())
        .collect();
    commands
}

fn path() -> std::path::PathBuf {
    dirs::config_dir().unwrap().join(".together.toml")
}
