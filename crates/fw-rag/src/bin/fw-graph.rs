// SPDX-License-Identifier: Apache-2.0
//! `fw-graph`: the code graph binary that `fw graph` runs (build, query, explain, path, report, export).

fn main() -> std::process::ExitCode {
    let code = fw_rag::codegraph::cli::cli_main(std::env::args_os().collect());
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}
