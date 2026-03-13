use crate::commands;

pub fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let mut args = args.into_iter();
    let _bin = args.next();

    match args.next() {
        Some(url) => commands::download::run(&url),
        None => {
            print_help();
            Ok(())
        }
    }
}

fn print_help() {
    println!("Lilith CLI");
    println!();
    println!("Usage:");
    println!("  lilith-cli <niconico-watch-url>");
    println!();
    println!("Example:");
    println!("  lilith-cli https://www.nicovideo.jp/watch/sm45174902");
}
