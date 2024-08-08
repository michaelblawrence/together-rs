use std::path::PathBuf;

use clap::CommandFactory;

use crate::{errors::TogetherResult, log, log_err, terminal};

pub struct StartTogetherOptions {
    pub arg_command: terminal::RunCommand,
    pub override_commands: Option<Vec<String>>,
    pub startup_commands: Option<Vec<String>>,
    pub working_directory: Option<String>,
    pub config_path: Option<std::path::PathBuf>,
}

pub fn to_start_options(args: terminal::TogetherArgs) -> StartTogetherOptions {
    let (config_start_opts, config) = match args.sub {
        Some(terminal::ArgsCommands::Run(run_opts)) => {
            let config_start_opts: commands::ConfigFileStartOptions = run_opts.into();
            (config_start_opts, None)
        }

        Some(terminal::ArgsCommands::Rerun(_)) => {
            if args.no_config {
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
            let config_path: PathBuf = path();
            (config.start_options.clone(), Some((config, config_path)))
        }

        Some(terminal::ArgsCommands::Load(load)) => {
            if args.no_config {
                log_err!("To use rerun, you must have a configuration file");
                std::process::exit(1);
            }
            let config = load_from(&load.path);
            let mut config = config
                .map_err(|e| {
                    log_err!("Failed to load configuration from '{}': {}", load.path, e);
                    std::process::exit(1);
                })
                .unwrap();
            let config_path: PathBuf = load.path.into();
            config.start_options.init_only = load.init_only;
            (config.start_options.clone(), Some((config, config_path)))
        }

        None => (!args.no_config)
            .then_some(())
            .and_then(|()| load_from("together.toml").ok())
            .map_or_else(
                || {
                    _ = terminal::TogetherArgs::command().print_long_help();
                    std::process::exit(1);
                },
                |config| {
                    let mut config_start_opts = config.start_options.clone();
                    config_start_opts.init_only = args.init_only;
                    (config_start_opts, Some((config, "together.toml".into())))
                },
            ),
    };

    let (running, startup, config_path) = match config {
        Some((config, config_path)) => {
            let commands = &config.start_options.commands;

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
                    .filter_map(|i| i.retrieve(commands).map(|c| c.as_str().to_string()))
                    .collect()
            });
            let running = Some(running).filter(|c| !c.is_empty());

            (running, startup, Some(config_path))
        }
        None => (None, None, None),
    };

    StartTogetherOptions {
        arg_command: config_start_opts.into(),
        override_commands: running,
        startup_commands: startup,
        working_directory: args.working_directory,
        config_path,
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TogetherConfigFile {
    #[serde(flatten)]
    pub start_options: commands::ConfigFileStartOptions,
    pub running: Option<Vec<commands::CommandIndex>>,
    pub startup: Option<Vec<commands::CommandIndex>>,
    pub version: Option<String>,
}

impl TogetherConfigFile {
    pub fn new(start_opts: &StartTogetherOptions, running: &[impl AsRef<str>]) -> Self {
        let running = running
            .iter()
            .map(|c| {
                start_opts
                    .arg_command
                    .commands
                    .iter()
                    .position(|x| x == c.as_ref())
                    .unwrap()
                    .into()
            })
            .collect();
        let startup = start_opts.startup_commands.as_ref().map(|commands| {
            commands
                .iter()
                .map(|c| {
                    start_opts
                        .arg_command
                        .commands
                        .iter()
                        .position(|x| x == c)
                        .unwrap()
                        .into()
                })
                .collect()
        });
        Self {
            start_options: start_opts.arg_command.clone().into(),
            running: Some(running),
            startup,
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }
    }
}

pub fn load_from(config_path: impl AsRef<std::path::Path>) -> TogetherResult<TogetherConfigFile> {
    let config = std::fs::read_to_string(config_path)?;
    let config: TogetherConfigFile = toml::from_str(&config)?;
    check_version(&config);
    Ok(config)
}

pub fn load() -> TogetherResult<TogetherConfigFile> {
    let config_path = path();
    log!("Loading configuration from: {:?}", config_path);
    load_from(config_path)
}

pub fn save(
    config: &TogetherConfigFile,
    config_path: Option<&std::path::Path>,
) -> TogetherResult<()> {
    let default_path = path();
    let config_path = config_path.unwrap_or_else(|| default_path.as_path());
    log!("Saving configuration to: {:?}", config_path);
    let config = toml::to_string_pretty(config)?;
    std::fs::write(config_path, config)?;
    Ok(())
}

pub fn dump(config: &TogetherConfigFile) -> TogetherResult<()> {
    let config = toml::to_string(config)?;
    println!("Configuration:");
    println!();
    println!("{}", config);
    Ok(())
}

pub fn get_running_commands(
    config: &TogetherConfigFile,
    running: &[commands::CommandIndex],
) -> Vec<String> {
    let commands: Vec<String> = running
        .iter()
        .filter_map(|index| {
            index
                .retrieve(&config.start_options.commands)
                .map(|c| c.as_str().to_string())
        })
        .collect();
    commands
}

fn path() -> std::path::PathBuf {
    dirs::config_dir().unwrap().join(".together.toml")
}

fn check_version(config: &TogetherConfigFile) {
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

pub mod commands {
    use serde::{Deserialize, Serialize};

    use crate::terminal;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ConfigFileStartOptions {
        pub commands: Vec<CommandConfig>,
        #[serde(default)]
        pub all: bool,
        #[serde(default)]
        pub exit_on_error: bool,
        #[serde(default)]
        pub quit_on_completion: bool,
        #[serde(default = "defaults::true_value")]
        pub raw: bool,
        #[serde(skip)]
        pub init_only: bool,
    }

    mod defaults {
        pub fn true_value() -> bool {
            true
        }
    }

    impl From<terminal::RunCommand> for ConfigFileStartOptions {
        fn from(args: terminal::RunCommand) -> Self {
            Self {
                commands: args.commands.iter().map(|c| c.as_str().into()).collect(),
                all: args.all,
                exit_on_error: args.exit_on_error,
                quit_on_completion: args.quit_on_completion,
                raw: args.raw,
                init_only: args.init_only,
            }
        }
    }

    impl From<ConfigFileStartOptions> for terminal::RunCommand {
        fn from(config: ConfigFileStartOptions) -> Self {
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
                init_only: config.init_only,
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

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum CommandIndex {
        Simple(usize),
        Alias(String),
    }

    impl CommandIndex {
        pub fn retrieve<'a>(&self, commands: &'a [CommandConfig]) -> Option<&'a CommandConfig> {
            match self {
                Self::Simple(i) => commands.get(*i),
                Self::Alias(alias) => commands
                    .iter()
                    .find(|c| c.alias() == Some(alias))
                    .or_else(|| commands.iter().find(|c| c.as_str() == alias)),
            }
        }
    }

    impl From<usize> for CommandIndex {
        fn from(v: usize) -> Self {
            Self::Simple(v)
        }
    }

    impl From<&str> for CommandIndex {
        fn from(v: &str) -> Self {
            Self::Alias(v.to_string())
        }
    }
}
