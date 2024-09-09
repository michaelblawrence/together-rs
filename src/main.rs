use clap::Parser;
use together_rs::{config, log_err, start, terminal, update};

fn main() {
    if let Err(e) = update::update() {
        log_err!("[warning] Failed to update together: {}", e);
    }
    let args = terminal::TogetherArgs::parse();
    let options = config::to_start_options(args);
    let result = start(options);
    if let Err(e) = result {
        log_err!("[fatal] Unexpected error: {}", e);
        std::process::exit(1);
    }
}
