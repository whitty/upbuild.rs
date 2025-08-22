// SPDX-License-Identifier: GPL-3.0-or-later
// (C) Copyright 2024-2025 Greg Whiteley

#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

use std::process::ExitCode;
use std::io::BufRead;

use upbuild_rs::{ClassicFile, Config, Exec, Result};

fn run() -> Result<()> {

    let (args, cfg) = Config::parse(std::env::args());

    if cfg.add() {
        return upbuild_rs::ClassicFile::add(args, ".upbuild".into());
    }

    let upbuild_file = upbuild_rs::find(".")?;

    let parsed_file = ClassicFile::parse_lines(
        std::fs::File::open(&upbuild_file)
            .map(std::io::BufReader::new)?
            .lines()
            .map_while(std::result::Result::ok))?;

    let exec = Exec::new(
        if cfg.print() {
            upbuild_rs::print_runner()
        } else {
            upbuild_rs::process_runner()
        }
    );

    let args: Vec<String> = args.collect(); // TODO - don't require conversion
    exec.run(upbuild_file.as_path(), &parsed_file, &cfg, &args)
}

fn main() -> ExitCode {
    match run() {
        Ok(_) => (),
        Err(upbuild_rs::Error::ExitWithExitCode(c)) => {
            match u8::try_from(c) {
                Ok(c) => {
                    return ExitCode::from(c);
                },
                Err(e) => {
                    if cfg!(windows) {
                        if let Ok(c) = i32::try_from(c) {
                            std::process::exit(c);
                        }
                    }
                    eprintln!("Unable to return process return code {}: {}", c, e);
                    return ExitCode::FAILURE;
                }
            }
        },
        Err(e) => {
            eprintln!("{}", e);
            return ExitCode::FAILURE;
        },
    };

    ExitCode::SUCCESS
}
