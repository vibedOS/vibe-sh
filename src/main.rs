// SPDX-License-Identifier: MIT

#![no_main]
#![no_std]

use core::ffi::{CStr, c_char};
use core::panic::PanicInfo;
use core::ptr;
use core::str;
use vibe_rt::{
    Args, Env, Errno, Fork, Result, entry, eprintln, execve, fork, getpid, print, read, reboot,
    wait_pid, write_all,
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
    let line = &input[..length];
    let mut words = line
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|word| !word.is_empty());
    let Some(command) = words.next() else {
        return true;
    };

    match command {
        b"help" => {
            vibe_rt::println!("builtins: help echo uname pid reboot exit");
            return true;
        }
        b"echo" => {
            for (index, word) in words.enumerate() {
                if index != 0 {
                    let _ = write_all(1, b" ");
                }
                let _ = write_all(1, word);
            }
            vibe_rt::println!();
            return true;
        }
        b"uname" => {
            vibe_rt::println!("vibeOS Linux 7.1.5 x86_64");
            return true;
        }
        b"pid" => {
            vibe_rt::println!("{}", getpid());
            return true;
        }
        b"reboot" => {
            if let Err(error) = reboot() {
                eprintln!("vibe-sh: reboot failed: errno {}", error.0);
            }
            return true;
        }
        b"exit" => return false,
        _ => {}
    }

    run_external(input, length);
    true
}

fn run_external(input: &mut [u8], length: usize) {
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
