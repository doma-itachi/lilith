mod cli;
mod commands;

#[tokio::main]
async fn main() {
    if let Err(error) = cli::run(std::env::args()).await {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
