// SPDX-License-Identifier: MIT

#![no_main]
#![no_std]

use core::panic::PanicInfo;
use core::str;
use vibe_rt::{Args, Env, Errno, Result, entry, eprintln, getpid, print, read, reboot, write_all};

entry!(main);

fn main(mut args: Args<'_>, _env: Env<'_>) -> i32 {
    let _program = args.next();
    if args.next() == Some(b"-c") {
        let Some(command) = args.next() else {
            eprintln!("vibe-sh: -c requires a command");
            return 2;
        };
        run(command);
        return 0;
    }

    vibe_rt::println!("vibe-sh 0.1");
    let mut input = [0_u8; 512];

    loop {
        print!("vibe$ ");
        match read_line(&mut input) {
            Ok(Some(line)) if !run(line) => return 0,
            Ok(Some(_)) => {}
            Ok(None) => return 0,
            Err(error) => eprintln!("vibe-sh: input error: errno {}", error.0),
        }
    }
}

fn read_line(buffer: &mut [u8]) -> Result<Option<&[u8]>> {
    let mut length = 0;

    loop {
        if length == buffer.len() {
            drain_line();
            return Err(Errno(7));
        }

        // ponytail: byte-wise reads are enough for a serial console; batch when throughput matters.
        let count = read(0, &mut buffer[length..length + 1])?;
        if count == 0 {
            return Ok((length != 0).then_some(&buffer[..length]));
        }
        if buffer[length] == b'\n' {
            return Ok(Some(&buffer[..length]));
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

fn run(line: &[u8]) -> bool {
    let mut words = line
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|word| !word.is_empty());
    let Some(command) = words.next() else {
        return true;
    };

    match command {
        b"help" => vibe_rt::println!("commands: help echo uname pid reboot exit"),
        b"echo" => {
            for (index, word) in words.enumerate() {
                if index != 0 {
                    let _ = write_all(1, b" ");
                }
                let _ = write_all(1, word);
            }
            vibe_rt::println!();
        }
        b"uname" => vibe_rt::println!("vibeOS Linux 7.1.5 x86_64"),
        b"pid" => vibe_rt::println!("{}", getpid()),
        b"reboot" => {
            if let Err(error) = reboot() {
                eprintln!("vibe-sh: reboot failed: errno {}", error.0);
            }
        }
        b"exit" => return false,
        unknown => eprintln!(
            "vibe-sh: command not found: {}",
            str::from_utf8(unknown).unwrap_or("<non-utf8>")
        ),
    }
    true
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    eprintln!("vibe-sh panic: {info}");
    vibe_rt::exit(101)
}
