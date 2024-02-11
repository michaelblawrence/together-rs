use dialoguer::{theme::ColorfulTheme, MultiSelect};

#[derive(Debug, clap::Parser)]
#[clap(
    name = "together",
    author = "Michael Lawrence",
    about = "Run multiple commands in parallel selectively by an interactive prompt."
)]
pub struct Opts {
    #[clap(subcommand)]
    pub sub: SubCommand,
}

#[derive(Debug, clap::Parser)]
pub enum SubCommand {
    #[clap(
        name = "run",
        about = "Run multiple commands in parallel selectively by an interactive prompt."
    )]
    Run(Run),
}

#[derive(Debug, clap::Parser)]
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
            .items(&items[..])
            .defaults(&defaults[..])
            .interact();
        let selections = multi_select.unwrap();
        for index in selections {
            opts_commands.push(&items[index]);
        }
        opts_commands
    }
    pub fn select_single<'a, T: std::fmt::Display>(prompt: &'a str, items: &'a [T]) -> &'a T {
        let index = dialoguer::Select::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .items(&items[..])
            .interact()
            .unwrap();
        &items[index]
    }
    pub fn log(message: &str) {
        // print message with green colorized prefix
        println!("\x1b[32m[+]\x1b[0m {}", message);
    }
    pub fn log_error(message: &str) {
        // print message with green colorized prefix
        eprintln!("\x1b[31m[!]\x1b[0m {}", message);
    }
}

// macro for logging like println! but with a green prefix
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        crate::terminal::Terminal::log(&format!($($arg)*));
    };
}
#[macro_export]
macro_rules! log_err {
    ($($arg:tt)*) => {
        crate::terminal::Terminal::log_error(&format!($($arg)*));
    };
}
