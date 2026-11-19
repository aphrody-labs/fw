#![forbid(unsafe_code)]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match fw_fx::cli::run_cli(&args) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("fx: {err}");
            std::process::exit(1);
        }
    }
}
