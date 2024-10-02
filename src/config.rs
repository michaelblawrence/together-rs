use std::{
    borrow::Cow,
    collections::HashSet,
    path::{Path, PathBuf},
};

use clap::CommandFactory;

use crate::{
    errors::{TogetherError, TogetherResult},
    log, log_err, t_println, terminal,
};

#[derive(Debug, Clone)]
pub struct StartTogetherOptions {
    pub config: TogetherConfigFile,
    pub working_directory: Option<String>,
    pub active_recipes: Option<Vec<String>>,
    pub config_path: Option<std::path::PathBuf>,
}

pub fn to_start_options(command_args: terminal::TogetherArgs) -> StartTogetherOptions {
    #[derive(Default)]
    struct StartMeta {
        config_path: Option<std::path::PathBuf>,
        recipes: Option<Vec<String>>,
    }
    let (config, meta) = match command_args.command {
        Some(terminal::ArgsCommands::Run(run_opts)) => {
            let mut config_start_opts: commands::ConfigFileStartOptions = run_opts.into();
            let meta = StartMeta::default();
            config_start_opts.init_only = command_args.init_only;
            config_start_opts.no_init = command_args.no_init;
            config_start_opts.quiet_startup = command_args.quiet_startup;
            (TogetherConfigFile::new(config_start_opts), meta)
        }

        Some(terminal::ArgsCommands::Rerun(_)) => {
            if command_args.no_config {
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
            let config_path: PathBuf = path_or_default();
            let meta = StartMeta {
                config_path: Some(config_path),
                ..StartMeta::default()
            };
            (config, meta)
        }

        Some(terminal::ArgsCommands::Load(load)) => {
            if command_args.no_config {
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
            config.start_options.no_init = load.no_init;
            config.start_options.quiet_startup = command_args.quiet_startup;
            let meta = StartMeta {
                config_path: Some(config_path),
                recipes: load.recipes,
            };
            (config, meta)
        }

        None => (!command_args.no_config)
            .then_some(())
            .and_then(|()| path(None))
            .and_then(|path| load_from(&path).ok().map(|config| (config, path)))
            .map_or_else(
                || {
                    _ = terminal::TogetherArgs::command().print_long_help();
                    std::process::exit(1);
                },
                |(mut config, config_path)| {
                    let config_start_opts = &mut config.start_options;
                    config_start_opts.init_only = command_args.init_only;
                    config_start_opts.no_init = command_args.no_init;
                    config_start_opts.quiet_startup = command_args.quiet_startup;
                    let meta = StartMeta {
                        config_path: Some(config_path.into()),
                        recipes: command_args.recipes,
                    };
                    (config, meta)
                },
            ),
    };

    StartTogetherOptions {
        config,
        working_directory: command_args.working_directory,
        active_recipes: meta.recipes,
        config_path: meta.config_path,
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TogetherConfigFile {
    #[serde(flatten)]
    pub start_options: commands::ConfigFileStartOptions,
    pub running: Option<Vec<commands::CommandIndex>>,
    pub startup: Option<Vec<commands::CommandIndex>>,
    pub version: Option<String>,
}

impl TogetherConfigFile {
    fn new(start_options: commands::ConfigFileStartOptions) -> Self {
        Self {
            start_options,
            running: None,
            startup: None,
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }
    }

    pub fn with_running(self, running: &[impl AsRef<str>]) -> Self {
        let running = running
            .iter()
            .map(|c| {
                self.start_options
                    .commands
                    .iter()
                    .position(|x| x.matches(c.as_ref()))
                    .unwrap()
                    .into()
            })
            .collect();

        Self {
            running: Some(running),
            ..self
        }
    }

    pub fn running_commands(&self) -> Option<Vec<&str>> {
        let running = self
            .running
            .iter()
            .flatten()
            .flat_map(|index| index.retrieve(&self.start_options.commands))
            .chain(self.start_options.commands.iter().filter(|c| c.is_active()))
            .fold(vec![], |mut acc, c| {
                if !acc.contains(&c) {
                    acc.push(c);
                }
                acc
            });

        if running.is_empty() {
            None
        } else {
            Some(running.into_iter().map(|c| c.as_str()).collect())
        }
    }
}

enum ConfigFileType {
    Toml,
    Yaml,
}

impl TryFrom<&std::path::Path> for ConfigFileType {
    type Error = TogetherError;

    fn try_from(value: &std::path::Path) -> Result<Self, Self::Error> {
        match value.extension().and_then(|ext| ext.to_str()) {
            Some("toml") => Ok(Self::Toml),
            Some("yaml") | Some("yml") => Ok(Self::Yaml),
            _ => Err(TogetherError::InternalError(
                crate::errors::TogetherInternalError::InvalidConfigExtension,
            )),
        }
    }
}

pub fn load_from(config_path: impl AsRef<std::path::Path>) -> TogetherResult<TogetherConfigFile> {
    let config_path = config_path.as_ref();
    let config = std::fs::read_to_string(config_path)?;
    let config: TogetherConfigFile = match config_path.try_into()? {
        ConfigFileType::Toml => toml::from_str(&config)?,
        ConfigFileType::Yaml => serde_yml::from_str(&config)?,
    };
    check_version(&config);
    Ok(config)
}

pub fn load() -> TogetherResult<TogetherConfigFile> {
    let config_path = path_or_default();
    log!("Loading configuration from: {:?}", config_path);
    load_from(config_path)
}

pub fn save(
    config: &TogetherConfigFile,
    config_path: Option<&std::path::Path>,
) -> TogetherResult<()> {
    let config_path = config_path
        .map(Cow::from)
        .unwrap_or_else(|| path_or_default().into());
    log!("Saving configuration to: {:?}", config_path);
    let config = match config_path.as_ref().try_into()? {
        ConfigFileType::Toml => toml::to_string(config)?,
        ConfigFileType::Yaml => serde_yml::to_string(config)?,
    };
    std::fs::write(config_path, config)?;
    Ok(())
}

pub fn dump(config: &TogetherConfigFile) -> TogetherResult<()> {
    let config = serde_yml::to_string(config)?;
    t_println!("Configuration:");
    t_println!();
    t_println!("{}", config);
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

pub fn get_unique_recipes(start_options: &commands::ConfigFileStartOptions) -> HashSet<&String> {
    start_options
        .commands
        .iter()
        .flat_map(|c| c.recipes())
        .collect::<HashSet<_>>()
}

pub fn collect_commands_by_recipes(
    start_options: &commands::ConfigFileStartOptions,
    recipes: &[impl AsRef<str>],
) -> Vec<String> {
    let selected_commands = start_options
        .commands
        .iter()
        .filter(|c| recipes.iter().any(|r| c.contains_recipe(r.as_ref())))
        .map(|c| c.as_str().to_string())
        .collect();
    selected_commands
}

fn path_or_default() -> std::path::PathBuf {
    let dir_path = dirs::config_dir().unwrap();
    match path(Some(&dir_path)) {
        Some(path) => path,
        None => dir_path.join("together.yml"),
    }
}

fn path(dir: Option<&Path>) -> Option<std::path::PathBuf> {
    let files = ["together.yml", "together.yaml", "together.toml"];
    files.iter().find_map(|f| {
        let path = match &dir {
            Some(dir) => dir.join(f),
            None => f.into(),
        };

        path.exists().then_some(path)
    })
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
            "The configuration file was created with a more recent version of together (>={config_version}). \
            Please update together to the latest version."
        );
        std::process::exit(1);
    }

    if current_version.minor < config_version.minor {
        log!(
            "Using configuration file created with a more recent version of together (>={config_version}). \
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
        #[serde(default)]
        pub quiet_startup: bool,
        #[serde(default = "defaults::true_value")]
        pub raw: bool,
        #[serde(skip)]
        pub init_only: bool,
        #[serde(skip)]
        pub no_init: bool,
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
                quiet_startup: false,
                raw: args.raw,
                init_only: args.init_only,
                no_init: args.no_init,
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
                no_init: config.no_init,
            }
        }
    }

    impl ConfigFileStartOptions {
        pub fn as_commands(&self) -> Vec<String> {
            self.commands
                .iter()
                .map(|c| c.as_str().to_string())
                .collect()
        }
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum CommandConfig {
        Simple(String),
        Detailed {
            command: String,
            alias: Option<String>,
            active: Option<bool>,
            recipes: Option<Vec<String>>,
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

        pub fn recipes(&self) -> &[String] {
            match self {
                Self::Simple(_) => &[],
                Self::Detailed { recipes, .. } => recipes.as_deref().unwrap_or(&[]),
            }
        }

        pub fn contains_recipe(&self, recipe: &str) -> bool {
            let recipe = recipe.trim();
            match self {
                Self::Simple(_) => false,
                Self::Detailed { recipes, .. } => recipes
                    .as_ref()
                    .map_or(false, |r| r.iter().any(|x| x.eq_ignore_ascii_case(recipe))),
            }
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
