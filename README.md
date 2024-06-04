# Clio A simple log redirector for stdout and stderr.

Clio redirects stdout and stderr for a subprocess to paths of your choosing.
When sent a configurable signal, SIGHUP by default, it will reopen the file
handles that it is redirecting the output to.

This is handy for use with logrotate or or other log rotation tools where they
can inform the clio that the logs have been rotated and it's file handles are
stale.

```sh
$> clio --help
A small log redirection utility

Usage: clio [OPTIONS] --err-path <STDERR_PATH> --out-path <STDOUT_PATH> [-- <CMD>...]

Arguments:
  [CMD]...  Command to run

Options:
  -e, --err-path <STDERR_PATH>  Path to write stderr to
  -o, --out-path <STDOUT_PATH>  Path to write stdout to
  -p, --pid-file <PID_FILE>     Path to the place to write a pidfile to
      --sig <ROTATED_SIGNAL>    Signal notifiying that the file paths have been rotated [default: sighup] [possible values: sighup, sigusr1, sigusr2]
  -h, --help                    Print help
  -V, --version                 Print version
```

## Example

```sh
clio --err-path /var/log/app.err.log --out-path /var/log/app.out.log --sig sighup --pid-file app.pid -- app --flag1 --flag2
```
