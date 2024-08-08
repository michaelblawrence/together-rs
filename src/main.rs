use clap::Parser;
use together_rs::{config, log_err, start, terminal};

fn main() {
    let args = terminal::TogetherArgs::parse();
    let options = config::to_start_options(args);
    let result = start(options);
    if let Err(e) = result {
        log_err!("Unexpected error: {}", e);
        std::process::exit(1);
    }
}
