use std::convert::From;
use std::path::PathBuf;
use std::process::{ExitCode, Stdio};

use anyhow;
use clap::{Parser, ValueEnum};
use tokio;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::signal::unix::{signal, SignalKind};

#[derive(Parser, Clone, ValueEnum)]
pub enum HandledSignals {
    SIGHUP,
    SIGUSR1,
    SIGUSR2,
}

impl From<&HandledSignals> for SignalKind {
    fn from(value: &HandledSignals) -> Self {
        match value {
            HandledSignals::SIGHUP => SignalKind::hangup(),
            HandledSignals::SIGUSR1 => SignalKind::user_defined1(),
            HandledSignals::SIGUSR2 => SignalKind::user_defined2(),
        }
    }
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short = 'e', long = "err-path", help = "Path to write stderr to")]
    stderr_path: PathBuf,
    #[arg(short = 'o', long = "out-path", help = "Path to write stdout to")]
    stdout_path: PathBuf,
    #[arg(long = "sig", value_enum, help="Signal notifiying that the file paths have been rotated", default_value_t = HandledSignals::SIGHUP)]
    rotated_signal: HandledSignals,
    #[arg(
        long = "size",
        help = "Optional size at which to rotate the files, conficts with --sig",
        conflicts_with = "rotated_signal"
    )]
    cmd: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    let args = Args::parse();
    let stderr_path = &args.stderr_path;
    let stdout_path = &args.stdout_path;
    // Setup our signal hook.
    let handled_sig: SignalKind = (&args.rotated_signal).into();
    let mut signal_iter = signal(handled_sig)?;
    // Setup our output wiring.
    let app_name = match args.cmd.first() {
        Some(n) => n,
        None => return Err(anyhow::anyhow!("No command specified")),
    };
    let child = Command::new(app_name)
        .args(args.cmd.into_iter().skip(1).collect::<Vec<String>>())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout_reader = child
        .stdout
        .expect("no valid stdout from command available");
    let mut stdout_buffer = [0; 8 * 1024];
    let mut stderr_reader = child
        .stderr
        .expect("no valid stderr from command available");
    let mut stderr_buffer = [0; 8 * 1024];

    let mut stderr_writer = File::options().append(true).open(stderr_path).await?;
    let mut stdout_writer = File::options().append(true).open(stdout_path).await?;
    loop {
        tokio::select! {
            // wait for a read on stdout
            out_result = stdout_reader.read(&mut stdout_buffer) => {
                match out_result {
                    Ok(n) => {
                        // TODO(zaphar): It is possible we should try to reopen the file if this
                        // write fails in some cases.
                        if let Err(e) = stdout_writer.write(&stdout_buffer[0..n]).await {
                            todo!();
                        }
                    },
                    Err(e) => {
                        // TODO(zaphar): This likely means the command has broken badly. We should
                        // do the right thing here..
                        todo!()
                    },
                }
            }
            // wait for a read on stderr
            err_result = stderr_reader.read(&mut stderr_buffer) => {
                match err_result {
                    Ok(n) => {
                        // TODO(zaphar): It is possible we should try to reopen the file if this
                        // write fails in some cases.
                        if let Err(e) = stderr_writer.write(&stderr_buffer[0..n]).await {
                            todo!()
                        }
                    },
                    Err(e) => {
                        // TODO(zaphar): This likely means the command has broken badly. We should
                        // do the right thing here..
                        todo!()
                    },
                }
            }
            _ = signal_iter.recv() => {
                // on sighub sync and reopen our files
                // NOTE(zaphar): This will cause the previously opened handles to get
                // dropped which will cause them to close assuming all the io has finished. This is why we sync
                // before reopening the files.
                // TODO(zaphar): These should do something in the event of an error
                _ = stderr_writer.sync_all().await;
                _ = stdout_writer.sync_all().await;
                stderr_writer = File::options().append(true).open(stderr_path).await?;
                stdout_writer = File::options().append(true).open(stdout_path).await?;
                // wait for a signal
            }
        }
    }
}
