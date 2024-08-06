use clap::Parser;
use together_rs::{config, log_err, start, terminal};

fn main() {
    let opts = terminal::Opts::parse();
    let context = config::to_start_options(opts);
    let result = start(context);
    if let Err(e) = result {
        log_err!("Unexpected error: {}", e);
        std::process::exit(1);
    }
}
