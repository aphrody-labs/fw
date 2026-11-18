//! `aysh`: the shell itself. The launcher is niubash's, built on the rubash engine; this binary only gives it
//! a name of its own.

use std::process::ExitCode;

fn main() -> ExitCode {
    niubash_host::main()
}
