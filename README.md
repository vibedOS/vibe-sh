# vibe-sh

`vibe-sh` is the interactive, libc-free Rust shell for vibeOS. It reads from
the serial console, implements a small set of built-ins, and executes commands
from `/bin`.

## Features

- interactive `vibe$` prompt
- non-interactive `-c` command mode
- external command execution with arguments
- current-directory changes inherited by later commands
- one `>` output redirection for built-ins and external commands
- a fixed 512-byte input line and 32-argument ceiling

## Built-ins

| Command | Purpose |
| --- | --- |
| `help` | list built-ins and installed vibeOS commands |
| `clear` | clear the terminal and return the cursor home |
| `echo TEXT...` | print text |
| `cd [DIRECTORY]` | change directory; defaults to `/` |
| `uname` | print `vibeOS Linux 7.1.5 x86_64` |
| `pid` | print the shell process ID |
| `reboot` | sync filesystems and reboot |
| `exit` | exit the shell |

vibeOS also installs `true`, `false`, `whoami`, `vibefetch`, `pwd`, `cat`,
`ls`, `mkdir`, `rm`, and `vibe-pkg` in `/bin`. Run `vibe-pkg list` to
discover executables installed later.

## Build and check

Rust 1.94.0 is selected by `rust-toolchain.toml`.

```sh
cargo build --release
target/x86_64-unknown-linux-gnu/release/vibe-sh -c 'echo hello'
target/x86_64-unknown-linux-gnu/release/vibe-sh -c 'echo saved > /tmp/vibe-output'
```

The release binary is statically linked and does not use libc. The repository
CI checks formatting, Clippy, the release build, binary linkage, built-ins,
and redirection.

## Scope

Parsing is intentionally minimal. Pipes, quoting, variables, globbing,
background jobs, input redirection, and PATH lookup are not implemented.

## License

MIT
