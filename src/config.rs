use crate::{errors::TogetherResult, log, log_err, terminal};

pub struct RunContext {
    pub opts: terminal::Run,
    pub override_commands: Option<Vec<String>>,
    pub startup_commands: Option<Vec<String>>,
    pub working_directory: Option<String>,
}

pub fn to_run_context(opts: terminal::Opts) -> RunContext {
    let (run_opts, config) = match opts.sub {
        terminal::SubCommand::Run(run_opts) => {
            let run_opts: commands::RunCommandsConfig = run_opts.into();
            (run_opts, None)
        }

        terminal::SubCommand::Rerun(_) => {
            if opts.no_config {
                log_err!("To use rerun, you must have a configuration file");
                std::process::exit(1);
            }
            let config = load();
            let config = config
                .map_err(|e| {
                    log_err!("Failed to load configuration: {}", e);
                    std::process::exit(1);
                })
                .unwrap();
            (config.run_opts.clone(), Some(config))
        }

        terminal::SubCommand::Load(load) => {
            if opts.no_config {
                log_err!("To use rerun, you must have a configuration file");
                std::process::exit(1);
            }
            let config = load_from(&load.path);
            let config = config
                .map_err(|e| {
                    log_err!("Failed to load configuration from '{}': {}", load.path, e);
                    std::process::exit(1);
                })
                .unwrap();
            (config.run_opts.clone(), Some(config))
        }
    };

    let (running, startup) = match config {
        Some(config) => {
            let commands = &config.run_opts.commands;

            let running = config.running.as_ref();
            let running = running
                .map(|running| get_running_commands(&config, running))
                .unwrap_or_else(|| {
                    let detailed_running = commands.iter().filter(|c| c.is_active());
                    let running = detailed_running.map(|c| c.as_str().to_string());
                    running.collect()
                });

            let startup = config.startup.as_ref().map(|startup| {
                startup
                    .iter()
                    .filter_map(|&i| commands.get(i).map(|c| c.as_str().to_string()))
                    .collect()
            });
            let running = Some(running).filter(|c| !c.is_empty());

            (running, startup)
        }
        None => (None, None),
    };

    RunContext {
        opts: run_opts.into(),
        override_commands: running,
        startup_commands: startup,
        working_directory: opts.working_directory,
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Config {
    #[serde(flatten)]
    pub run_opts: commands::RunCommandsConfig,
    pub running: Option<Vec<usize>>,
    pub startup: Option<Vec<usize>>,
    pub version: Option<String>,
}

impl Config {
    pub fn new(context: &RunContext, running: &[impl AsRef<str>]) -> Self {
        let running = running
            .iter()
            .map(|c| {
                context
                    .opts
                    .commands
                    .iter()
                    .position(|x| x == c.as_ref())
                    .unwrap()
            })
            .collect();
        let startup = context.startup_commands.as_ref().map(|commands| {
            commands
                .iter()
                .map(|c| context.opts.commands.iter().position(|x| x == c).unwrap())
                .collect()
        });
        Self {
            run_opts: context.opts.clone().into(),
            running: Some(running),
            startup,
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }
    }
}

pub fn load_from(config_path: impl AsRef<std::path::Path>) -> TogetherResult<Config> {
    let config = std::fs::read_to_string(config_path)?;
    let config: Config = toml::from_str(&config)?;
    check_version(&config);
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
        .filter_map(|index| {
            config
                .run_opts
                .commands
                .get(*index)
                .map(|c| c.as_str().to_string())
        })
        .collect();
    commands
}

fn path() -> std::path::PathBuf {
    dirs::config_dir().unwrap().join(".together.toml")
}

fn check_version(config: &Config) {
    let Some(version) = &config.version else {
        log_err!(
            "The configuration file was created with a different version of together. \
            Please update together to the latest version."
        );
        std::process::exit(1);
    };
    let current_version = env!("CARGO_PKG_VERSION");
    let current_version = semver::Version::parse(current_version).unwrap();
    let config_version = semver::Version::parse(version).unwrap();
    if current_version.major < config_version.major {
        log_err!(
            "The configuration file was created with a more recent version of together. \
            Please update together to the latest version."
        );
        std::process::exit(1);
    }

    if current_version.minor < config_version.minor {
        log!(
            "Using configuration file created with a more recent version of together. \
            Some features may not be available."
        );
    }
}

mod commands {
    use serde::{Deserialize, Serialize};

    use crate::terminal;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RunCommandsConfig {
        pub commands: Vec<CommandConfig>,
        pub all: bool,
        pub exit_on_error: bool,
        pub quit_on_completion: bool,
        pub raw: bool,
    }

    impl From<terminal::Run> for RunCommandsConfig {
        fn from(run: terminal::Run) -> Self {
            Self {
                commands: run.commands.iter().map(|c| c.as_str().into()).collect(),
                all: run.all,
                exit_on_error: run.exit_on_error,
                quit_on_completion: run.quit_on_completion,
                raw: run.raw,
            }
        }
    }

    impl From<RunCommandsConfig> for terminal::Run {
        fn from(config: RunCommandsConfig) -> Self {
            Self {
                commands: config
                    .commands
                    .iter()
                    .map(|c| c.as_str().to_string())
                    .collect(),
                all: config.all,
                exit_on_error: config.exit_on_error,
                quit_on_completion: config.quit_on_completion,
                raw: config.raw,
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum CommandConfig {
        Simple(String),
        Detailed {
            command: String,
            alias: Option<String>,
            active: Option<bool>,
        },
    }

    impl CommandConfig {
        pub fn as_str(&self) -> &str {
            match self {
                Self::Simple(s) => s,
                Self::Detailed { command, .. } => command,
            }
        }

        pub fn alias(&self) -> Option<&str> {
            match self {
                Self::Simple(_) => None,
                Self::Detailed { alias, .. } => alias.as_deref(),
            }
        }

        pub fn is_active(&self) -> bool {
            match self {
                Self::Simple(_) => false,
                Self::Detailed { active, .. } => active.unwrap_or(false),
            }
        }

        pub fn matches(&self, other: &str) -> bool {
            self.as_str() == other || self.alias().map_or(false, |a| a == other)
        }
    }

    impl From<&str> for CommandConfig {
        fn from(v: &str) -> Self {
            Self::Simple(v.to_string())
        }
    }
}
