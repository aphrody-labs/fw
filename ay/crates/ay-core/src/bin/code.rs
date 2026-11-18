use clap::Parser;

fn main() {
    std::process::exit(ay_core::cli::run(ay_core::cli::Cli::parse().cmd));
}
