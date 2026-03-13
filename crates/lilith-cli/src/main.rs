mod cli;
mod commands;

fn main() {
    if let Err(error) = cli::run(std::env::args()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
