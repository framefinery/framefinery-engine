use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    framefinery::run(env::args_os())
}
