use dialoguer::{theme::ColorfulTheme, MultiSelect};
use termion::color;

#[derive(Debug, clap::Parser)]
#[clap(
    name = "together",
    author = "Michael Lawrence",
    about = "Run multiple commands in parallel selectively by an interactive prompt."
)]
pub struct Opts {
    #[clap(subcommand)]
    pub sub: Option<SubCommand>,

    #[clap(short, long, help = "Ignore configuration file.")]
    pub no_config: bool,

    #[clap(short, long = "cwd", help = "Directory to run commands in.")]
    pub working_directory: Option<String>,

    #[clap(short, long, help = "Only run the startup commands.")]
    pub init_only: bool,
}

#[derive(Debug, clap::Parser)]
pub enum SubCommand {
    #[clap(
        name = "run",
        about = "Run multiple commands in parallel selectively by an interactive prompt."
    )]
    Run(Run),

    #[clap(name = "rerun", about = "Rerun the last together session.")]
    Rerun(Rerun),

    #[clap(name = "load", about = "Run commands from a configuration file.")]
    Load(Load),
}

#[derive(Debug, clap::Parser)]
pub struct Load {
    #[clap(required = true, help = "Configuration file path.")]
    pub path: String,

    #[clap(short, long, help = "Only run the startup commands.")]
    pub init_only: bool,
}

#[derive(Debug, clap::Parser)]
pub struct Rerun {}

#[derive(Debug, Clone, clap::Parser)]
pub struct Run {
    #[clap(
        last = true,
        required = true,
        help = "Commands to run. e.g. 'ls -l', 'echo hello'"
    )]
    pub commands: Vec<String>,

    #[clap(short, long, help = "Run all commands without prompting.")]
    pub all: bool,

    #[clap(
        short,
        long,
        help = "Exit on the first command that exits with a non-zero status."
    )]
    pub exit_on_error: bool,

    #[clap(
        short,
        long,
        help = "Quit the program when all commands have completed."
    )]
    pub quit_on_completion: bool,

    #[clap(short, long, help = "Enable raw stdout/stderr output.")]
    pub raw: bool,

    #[clap(short, long, help = "Only run the startup commands.")]
    pub init_only: bool,
}

pub struct Terminal;

impl Terminal {
    pub fn select_multiple<'a, T: std::fmt::Display>(
        prompt: &'a str,
        items: &'a [T],
    ) -> Vec<&'a T> {
        let mut opts_commands = vec![];
        let defaults = items.iter().map(|_| false).collect::<Vec<_>>();
        let multi_select = MultiSelect::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .items(items)
            .defaults(&defaults[..])
            .interact();
        let selections = multi_select.map_err(map_dialoguer_err).unwrap();
        for index in selections {
            opts_commands.push(&items[index]);
        }
        opts_commands
    }
    pub fn select_single<'a, T: std::fmt::Display>(
        prompt: &'a str,
        items: &'a [T],
    ) -> Option<&'a T> {
        let index = dialoguer::Select::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .items(items)
            .interact_opt()
            .map_err(map_dialoguer_err)
            .unwrap()?;
        Some(&items[index])
    }
    pub fn select_ordered<'a, T: std::fmt::Display>(
        prompt: &'a str,
        items: &'a [T],
    ) -> Option<Vec<&'a T>> {
        let mut opts_commands = vec![];
        let sort = dialoguer::Sort::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .items(items)
            .interact_opt()
            .map_err(map_dialoguer_err)
            .unwrap()?;
        for index in sort {
            opts_commands.push(&items[index]);
        }
        Some(opts_commands)
    }
    pub fn log(message: &str) {
        // print message with green colorized prefix
        println!(
            "{}[+] {}{}",
            color::Fg(color::Green),
            color::Fg(color::Reset),
            message
        );
    }
    pub fn log_error(message: &str) {
        // print message with red colorized prefix
        eprintln!(
            "{}[!] {}{}",
            color::Fg(color::Red),
            color::Fg(color::Reset),
            message
        );
    }
}

fn map_dialoguer_err(err: dialoguer::Error) -> ! {
    let dialoguer::Error::IO(io) = err;
    match io.kind() {
        std::io::ErrorKind::Interrupted | std::io::ErrorKind::BrokenPipe => {
            std::process::exit(0);
        }
        _ => {
            panic!("Unexpected error: {}", io);
        }
    }
}

// macro for logging like println! but with a green prefix
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::terminal::Terminal::log(&format!($($arg)*));
    };
}
#[macro_export]
macro_rules! log_err {
    ($($arg:tt)*) => {
        $crate::terminal::Terminal::log_error(&format!($($arg)*));
    };
}
