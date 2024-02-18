use crate::{errors::TogetherResult, log, log_err, terminal};

pub fn to_run_opts(opts: terminal::Opts) -> (terminal::Run, Option<Vec<String>>) {
    let (run_opts, selected_commands) = match opts.sub {
        terminal::SubCommand::Run(run_opts) => (run_opts, None),

        terminal::SubCommand::Rerun(_) => {
            if opts.no_config {
                log_err!("To use rerun, you must have a configuration file");
                std::process::exit(1);
            }
            match load() {
                Ok(config) => {
                    let commands = get_running_commands(&config, &config.running);
                    (config.run_opts, Some(commands).filter(|c| !c.is_empty()))
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
                    (config.run_opts, Some(commands).filter(|c| !c.is_empty()))
                }
                Err(e) => {
                    log_err!("Failed to load configuration: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };
    (run_opts, selected_commands)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub run_opts: crate::terminal::Run,
    pub running: Vec<usize>,
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
