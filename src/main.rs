use std::ffi::c_int;
use std::process;
use std::convert::From;
use std::path::PathBuf;
use signal_hook::consts::signal::*;

use clap::{Parser, ValueEnum};
use anyhow;

#[derive(Parser, Clone, ValueEnum)]
pub enum HandledSignals {
    SIGHUP,
    SIGUSR1,
    SIGUSR2,
}

impl From<HandledSignals> for c_int {
    fn from(value: HandledSignals) -> Self {
        match value {
            HandledSignals::SIGHUP => SIGHUP,
            HandledSignals::SIGUSR1 => SIGUSR1,
            HandledSignals::SIGUSR2 => SIGUSR2,
        }
    }
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short = 'e', long = "err-path", help="Path to write stderr to")]
    std_err_path: PathBuf,
    #[arg(short = 'o', long = "out-path", help="Path to write stdout to")]
    std_out_path: PathBuf,
    #[arg(long = "sig", value_enum, help="Optional signal notifiying that the file paths have been rotated")]
    rotated_signal: Option<HandledSignals>,
    #[arg(long="size", help="Optional size at which to rotate the files")]
    rotate_size_bytes: Option<usize>,
    #[arg(last = true, help="Command to run")]
    cmd: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    println!("Hello, world!");
    return Ok(());
}
