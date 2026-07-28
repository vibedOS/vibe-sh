// SPDX-License-Identifier: MIT

#![no_main]
#![no_std]

use core::ffi::{CStr, c_char};
use core::panic::PanicInfo;
use core::ptr;
use core::str;
use vibe_rt::{
    Args, Env, Errno, Fork, Result, change_dir, close, duplicate_to, entry, eprintln, execve, fork,
    getpid, open_write, print, read, reboot, wait_pid, write_all,
};

entry!(main);

fn main(mut args: Args<'_>, _env: Env<'_>) -> i32 {
    let _program = args.next();
    if args.next() == Some(b"-c") {
        let Some(command) = args.next() else {
            eprintln!("vibe-sh: -c requires a command");
            return 2;
        };
        let mut input = [0_u8; 512];
        if command.len() >= input.len() {
            eprintln!("vibe-sh: command too long");
            return 2;
        }
        input[..command.len()].copy_from_slice(command);
        run(&mut input, command.len());
        return 0;
    }

    vibe_rt::println!("vibe-sh 0.1");
    let mut input = [0_u8; 512];

    loop {
        print!("vibe$ ");
        match read_line(&mut input) {
            Ok(Some(length)) if !run(&mut input, length) => return 0,
            Ok(Some(_)) => {}
            Ok(None) => return 0,
            Err(error) => eprintln!("vibe-sh: input error: errno {}", error.0),
        }
    }
}

fn read_line(buffer: &mut [u8]) -> Result<Option<usize>> {
    let mut length = 0;

    loop {
        if length + 1 == buffer.len() {
            drain_line();
            return Err(Errno(7));
        }

        // ponytail: byte-wise reads are enough for a serial console; batch when throughput matters.
        let count = read(0, &mut buffer[length..length + 1])?;
        if count == 0 {
            return Ok((length != 0).then_some(length));
        }
        if buffer[length] == b'\n' {
            return Ok(Some(length));
        }
        if buffer[length] != b'\r' {
            length += 1;
        }
    }
}

fn drain_line() {
    let mut byte = [0_u8; 1];
    while read(0, &mut byte) == Ok(1) && byte[0] != b'\n' {}
}

fn run(input: &mut [u8], length: usize) -> bool {
    let (length, redirect) = match redirection(&input[..length]) {
        Ok(redirection) => redirection,
        Err(()) => {
            eprintln!("vibe-sh: usage: COMMAND > FILE");
            return true;
        }
    };
    let output = match redirect {
        Some(path) => {
            let mut storage = [0_u8; 512];
            let Some(path) = argument_path(path, &mut storage) else {
                eprintln!("vibe-sh: redirect path too long");
                return true;
            };
            match open_write(path) {
                Ok(output) => output,
                Err(error) => {
                    eprintln!("vibe-sh: redirect failed: errno {}", error.0);
                    return true;
                }
            }
        }
        None => 1,
    };

    let line = &input[..length];
    let mut words = line
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|word| !word.is_empty());
    let Some(command) = words.next() else {
        if output != 1 {
            let _ = close(output);
        }
        return true;
    };

    let keep_running = match command {
        b"help" => {
            let _ = write_all(
                output as usize,
                b"builtins: help clear echo cd uname pid reboot exit\n\
commands: true false whoami vibefetch pwd cat ls mkdir rm vibe-pkg\n\
packages: run vibe-pkg list\n",
            );
            true
        }
        b"clear" => {
            let _ = write_all(output as usize, b"\x1b[2J\x1b[H");
            true
        }
        b"echo" => {
            for (index, word) in words.enumerate() {
                if index != 0 {
                    let _ = write_all(output as usize, b" ");
                }
                let _ = write_all(output as usize, word);
            }
            let _ = write_all(output as usize, b"\n");
            true
        }
        b"cd" => {
            let directory = words.next().unwrap_or(b"/");
            if words.next().is_some() {
                eprintln!("usage: cd [DIRECTORY]");
            } else {
                let mut storage = [0_u8; 512];
                if let Some(directory) = argument_path(directory, &mut storage) {
                    if let Err(error) = change_dir(directory) {
                        eprintln!("cd: errno {}", error.0);
                    }
                } else {
                    eprintln!("cd: path too long");
                }
            }
            true
        }
        b"uname" => {
            let _ = write_all(output as usize, b"vibeOS Linux 7.1.5 x86_64\n");
            true
        }
        b"pid" => {
            print(format_args!("{}\n", getpid()), output as usize);
            true
        }
        b"reboot" => {
            if let Err(error) = reboot() {
                eprintln!("vibe-sh: reboot failed: errno {}", error.0);
            }
            true
        }
        b"exit" => false,
        _ => {
            drop(words);
            run_external(input, length, output);
            true
        }
    };

    if output != 1 {
        let _ = close(output);
    }
    keep_running
}

fn redirection(line: &[u8]) -> core::result::Result<(usize, Option<&[u8]>), ()> {
    let Some(marker) = line.iter().position(|byte| *byte == b'>') else {
        return Ok((line.len(), None));
    };
    if line[marker + 1..].contains(&b'>') {
        return Err(());
    }

    let mut command_end = marker;
    while command_end != 0 && line[command_end - 1].is_ascii_whitespace() {
        command_end -= 1;
    }
    let target = line[marker + 1..]
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|word| !word.is_empty());
    let mut target = target;
    let Some(path) = target.next() else {
        return Err(());
    };
    if command_end == 0 || target.next().is_some() {
        return Err(());
    }
    Ok((command_end, Some(path)))
}

fn argument_path<'a>(value: &[u8], storage: &'a mut [u8]) -> Option<&'a CStr> {
    if value.len() >= storage.len() {
        return None;
    }
    storage[..value.len()].copy_from_slice(value);
    storage[value.len()] = 0;
    CStr::from_bytes_with_nul(&storage[..=value.len()]).ok()
}

fn run_external(input: &mut [u8], length: usize, output: i32) {
    input[length] = 0;
    // ponytail: 32 arguments cover the bring-up shell; raise this fixed ceiling if real usage needs it.
    let mut arguments = [ptr::null::<c_char>(); 33];
    let mut count = 0;
    let mut in_word = false;

    for byte in &mut input[..=length] {
        if *byte == 0 || byte.is_ascii_whitespace() {
            *byte = 0;
            in_word = false;
        } else if !in_word {
            if count == arguments.len() - 1 {
                eprintln!("vibe-sh: too many arguments");
                return;
            }
            arguments[count] = ptr::from_ref(byte).cast();
            count += 1;
            in_word = true;
        }
    }

    // SAFETY: Parsing above produced at least one NUL-terminated argument.
    let command = unsafe { CStr::from_ptr(arguments[0]) };
    let mut path = [0_u8; 512];
    let prefix = b"/bin/";
    if prefix.len() + command.count_bytes() + 1 > path.len() {
        eprintln!("vibe-sh: command too long");
        return;
    }
    path[..prefix.len()].copy_from_slice(prefix);
    path[prefix.len()..prefix.len() + command.count_bytes()].copy_from_slice(command.to_bytes());
    let path_length = prefix.len() + command.count_bytes() + 1;
    let path = CStr::from_bytes_with_nul(&path[..path_length]).expect("constructed command path");
    let environment = [ptr::null::<c_char>()];

    match fork() {
        Ok(Fork::Child) => {
            if output != 1 {
                if let Err(error) = duplicate_to(output, 1) {
                    eprintln!("vibe-sh: redirect failed: errno {}", error.0);
                    vibe_rt::exit(1)
                }
                let _ = close(output);
            }
            // SAFETY: Both arrays are NUL-terminated and point into live stack buffers.
            if let Err(error) = unsafe { execve(path, arguments.as_ptr(), environment.as_ptr()) } {
                eprintln!(
                    "vibe-sh: command not found: {} (errno {})",
                    str::from_utf8(command.to_bytes()).unwrap_or("<non-utf8>"),
                    error.0
                );
            }
            vibe_rt::exit(127)
        }
        Ok(Fork::Parent(pid)) => {
            if let Err(error) = wait_pid(pid) {
                eprintln!("vibe-sh: wait failed: errno {}", error.0);
            }
        }
        Err(error) => eprintln!("vibe-sh: fork failed: errno {}", error.0),
    }
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    eprintln!("vibe-sh panic: {info}");
    vibe_rt::exit(101)
}
