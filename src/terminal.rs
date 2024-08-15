use dialoguer::{theme::ColorfulTheme, MultiSelect};
use termion::color;

#[derive(Debug, clap::Parser)]
#[clap(
    name = "together",
    author = "Michael Lawrence",
    about = "Run multiple commands in parallel selectively by an interactive prompt."
)]
pub struct TogetherArgs {
    #[clap(subcommand)]
    pub command: Option<ArgsCommands>,

    #[clap(short, long, help = "Ignore configuration file.")]
    pub no_config: bool,

    #[clap(short, long = "cwd", help = "Directory to run commands in.")]
    pub working_directory: Option<String>,

    #[clap(short, long, help = "Only run the startup commands.")]
    pub init_only: bool,

    #[clap(
        short,
        long,
        help = "Run all commands tagged under provided recipe(s). Use comma to separate multiple recipes.",
        value_delimiter = ','
    )]
    pub recipes: Option<Vec<String>>,
}

#[derive(Debug, clap::Parser)]
pub enum ArgsCommands {
    #[clap(
        name = "run",
        about = "Run multiple commands in parallel selectively by an interactive prompt."
    )]
    Run(RunCommand),

    #[clap(name = "rerun", about = "Rerun the last together session.")]
    Rerun(RerunCommand),

    #[clap(name = "load", about = "Run commands from a configuration file.")]
    Load(LoadCommand),
}

#[derive(Debug, clap::Parser)]
pub struct LoadCommand {
    #[clap(required = true, help = "Configuration file path.")]
    pub path: String,

    #[clap(short, long, help = "Only run the startup commands.")]
    pub init_only: bool,

    #[clap(
        short,
        long,
        help = "Run all commands tagged under provided recipe(s). Use comma to separate multiple recipes.",
        value_delimiter = ','
    )]
    pub recipes: Option<Vec<String>>,
}

#[derive(Debug, clap::Parser)]
pub struct RerunCommand {}

#[derive(Debug, Clone, clap::Parser)]
pub struct RunCommand {
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
        let index = Self::select_single_index(prompt, items)?;
        Some(&items[index])
    }
    pub fn select_single_index<'a, T: std::fmt::Display>(
        prompt: &'a str,
        items: &'a [T],
    ) -> Option<usize> {
        let index = dialoguer::Select::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .items(items)
            .interact_opt()
            .map_err(map_dialoguer_err)
            .unwrap()?;
        Some(index)
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
        crate::t_println!(
            "{}[+] {}{}",
            color::Fg(color::Green),
            color::Fg(color::Reset),
            message
        );
    }
    pub fn log_error(message: &str) {
        // print message with red colorized prefix
        crate::t_eprintln!(
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

pub mod stdout {
    /// macro for logging like println! but with a carriage return
    #[macro_export]
    macro_rules! t_println {
        () => {
            ::std::print!("\r\n");
        };
        ($fmt:tt) => {
            ::std::print!(concat!($fmt, "\r\n"));
        };
        ($fmt:tt, $($arg:tt)*) => {
            ::std::print!(concat!($fmt, "\r\n"), $($arg)*);
        };
    }

    /// macro for logging like eprintln! but with a carriage return
    #[macro_export]
    macro_rules! t_eprintln {
        () => {
            ::std::eprint!("\r\n");
        };
        ($fmt:tt) => {
            ::std::eprint!(concat!($fmt, "\r\n"));
        };
        ($fmt:tt, $($arg:tt)*) => {
            ::std::eprint!(concat!($fmt, "\r\n"), $($arg)*);
        };
    }
}

/// macro for logging like println! but with a green prefix
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::terminal::Terminal::log(&format!($($arg)*));
    };
}

/// macro for logging like eprintln! but with a red prefix
#[macro_export]
macro_rules! log_err {
    ($($arg:tt)*) => {
        $crate::terminal::Terminal::log_error(&format!($($arg)*));
    };
}
