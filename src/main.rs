use clap::Parser;

use re_typ::cli::Cli;

fn main() {
    let cli = Cli::parse();
    match re_typ::run(cli) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("error: {err:#}");
            std::process::exit(1);
        }
    }
}
