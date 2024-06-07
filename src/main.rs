use std::convert::From;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::process::{ExitCode, ExitStatus, Stdio};

use anyhow;
use clap::{Parser, ValueEnum};
use tokio;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::signal::unix::{signal, SignalKind};

#[derive(Parser, Clone, ValueEnum, Debug)]
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

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short = 'e', long = "err-path", help = "Path to write stderr to")]
    stderr_path: PathBuf,
    #[arg(short = 'o', long = "out-path", help = "Path to write stdout to")]
    stdout_path: PathBuf,
    #[arg(
        short = 'p',
        long = "pid-file",
        help = "Path to the place to write a pidfile to"
    )]
    pid_file: Option<PathBuf>,
    #[arg(long = "sig", value_enum, help="Signal notifiying that the file paths have been rotated", default_value_t = HandledSignals::SIGHUP)]
    rotated_signal: HandledSignals,
    #[arg(last = true, help = "Command to run")]
    cmd: Vec<String>,
}

async fn write_pid_file(p: &PathBuf) -> anyhow::Result<()> {
    let mut pid_file = File::create(p).await?;
    let id = std::process::id().to_string();
    pid_file.write(id.as_bytes()).await?;
    pid_file.sync_all().await?;
    Ok(())
}

async fn cleanup(
    result: std::io::Result<ExitStatus>,
    pid_file: &Option<PathBuf>,
) -> anyhow::Result<ExitCode> {
    if let Some(p) = pid_file {
        tokio::fs::remove_file(p).await?;
    }
    return Ok(ExitCode::from(
        result?.code().expect("No exit code for process") as u8,
    ));
}

fn check_for_stale_handle(f: &File) -> anyhow::Result<bool> {
    let fd = f.as_raw_fd();
    let stats = nix::sys::stat::fstat(fd)?;
    return Ok(stats.st_nlink > 0);
}

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    let args = Args::parse();
    let stderr_path = &args.stderr_path;
    let stdout_path = &args.stdout_path;
    // Setup our signal hook.
    let handled_sig: SignalKind = (&args.rotated_signal).into();
    let mut rotation_signal_stream = signal(handled_sig)?;
    let mut sigterm_stream = signal(SignalKind::terminate())?;
    let mut sigquit_stream = signal(SignalKind::quit())?;
    let mut sigint_stream = signal(SignalKind::interrupt())?;
    // Setup our output wiring.
    let app_name = match args.cmd.first() {
        Some(n) => n,
        None => return Err(anyhow::anyhow!("No command specified")),
    };
    let mut child = Command::new(app_name)
        .args(args.cmd.into_iter().skip(1).collect::<Vec<String>>())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout_reader = child
        .stdout
        .take()
        .expect("no valid stdout from command available");
    let mut stdout_buffer = [0; 8 * 1024];
    let mut stderr_reader = child
        .stderr
        .take()
        .expect("no valid stderr from command available");
    let mut stderr_buffer = [0; 8 * 1024];

    let mut stderr_writer = File::options()
        .append(true)
        .create(true)
        .open(stderr_path)
        .await?;
    let mut stdout_writer = File::options()
        .append(true)
        .create(true)
        .open(stdout_path)
        .await?;
    // TODO(jwall): Write our pidfile somehwere
    if let Some(p) = &args.pid_file {
        write_pid_file(p).await?
    }
    // TODO(jwall): Forward all other signals to the running process.
    loop {
        // NOTE(zaphar): Each select block will run exclusively of the other blocks using a
        // psuedorandom order.
        tokio::select! {
            // wait for a read on stdout
            out_result = stdout_reader.read(&mut stdout_buffer) => {
                match out_result {
                    Ok(n) => {
                        if !check_for_stale_handle(&stdout_writer)? {
                            stdout_writer.flush().await?;
                            stdout_writer = File::options().append(true).create(true).open(stdout_path).await?;
                        }
                        if let Err(_) = stdout_writer.write(&stdout_buffer[0..n]).await {
                            stdout_writer.flush().await?;
                            stdout_writer = File::options().append(true).create(true).open(stdout_path).await?;
                        }
                    },
                    Err(_) => {
                        // TODO(zaphar): This likely means the command has broken badly. We should
                        // do the right thing here.
                        let result = child.wait().await;
                        return cleanup(result, &args.pid_file).await;
                    },
                }
            }
            // wait for a read on stderr
            err_result = stderr_reader.read(&mut stderr_buffer) => {
                match err_result {
                    Ok(n) => {
                        if !check_for_stale_handle(&stderr_writer)? {
                            stderr_writer.flush().await?;
                            stderr_writer = File::options().append(true).create(true).open(stderr_path).await?;
                        }
                        if let Err(_) = stderr_writer.write(&stderr_buffer[0..n]).await {
                            stderr_writer.flush().await?;
                            stderr_writer = File::options().append(true).create(true).open(stderr_path).await?;
                        }
                    },
                    Err(_) => {
                        // TODO(zaphar): This likely means the command has broken badly. We should
                        // do the right thing here..
                        let result = child.wait().await;
                        return cleanup(result, &args.pid_file).await;
                    },
                }
            }
            _ = rotation_signal_stream.recv() => {
                // on sighub sync and reopen our files
                // NOTE(zaphar): This will cause the previously opened handles to get
                // dropped which will cause them to close assuming all the io has finished. This is why we sync
                // before reopening the files.
                // TODO(zaphar): These should do something in the event of an error
                _ = stderr_writer.sync_all().await;
                _ = stdout_writer.sync_all().await;
                stderr_writer = File::options().append(true).create(true).open(stderr_path).await?;
                stdout_writer = File::options().append(true).create(true).open(stdout_path).await?;
            }
            _ = sigterm_stream.recv() => {
                // NOTE(zaphar): This is a giant hack.
                // If https://github.com/tokio-rs/tokio/issues/3379 ever get's implemented it will become
                // unnecessary.
                use nix::{
                    sys::signal::{kill, Signal::SIGTERM},
                    unistd::Pid,
                };
                if let Some(pid) = child.id() {
                    // If the child hasn't already completed, send a SIGTERM.
                    if let Err(e) = kill(Pid::from_raw(pid.try_into().expect("Invalid PID")), SIGTERM) {
                        eprintln!("Failed to forward SIGTERM to child process: {}", e);
                    }
                }
            }
            _ = sigquit_stream.recv() => {
                // NOTE(zaphar): This is a giant hack.
                // If https://github.com/tokio-rs/tokio/issues/3379 ever get's implemented it will become
                // unnecessary.
                use nix::{
                    sys::signal::{kill, Signal::SIGQUIT},
                    unistd::Pid,
                };
                if let Some(pid) = child.id() {
                    // If the child hasn't already completed, send a SIGTERM.
                    if let Err(e) = kill(Pid::from_raw(pid.try_into().expect("Invalid PID")), SIGQUIT) {
                        eprintln!("Failed to forward SIGQUIT to child process: {}", e);
                    }
                }
            }
            _ = sigint_stream.recv() => {
                // NOTE(zaphar): This is a giant hack.
                // If https://github.com/tokio-rs/tokio/issues/3379 ever get's implemented it will become
                // unnecessary.
                use nix::{
                    sys::signal::{kill, Signal::SIGINT},
                    unistd::Pid,
                };
                if let Some(pid) = child.id() {
                    // If the child hasn't already completed, send a SIGTERM.
                    if let Err(e) = kill(Pid::from_raw(pid.try_into().expect("Invalid PID")), SIGINT) {
                        eprintln!("Failed to forward SIGINT to child process: {}", e);
                    }
                }
            }
            result = child.wait() => {
                // The child has finished
                return cleanup(result, &args.pid_file).await;
            }
        }
    }
}
