use std::process::ExitCode;
use std::io::BufRead;

use upbuild_rs::{ClassicFile, Config, Exec, Result};

fn run() -> Result<()> {

    let (args, cfg) = Config::parse(std::env::args().skip(1)); // skip argv[0]

    // TODO handle args
    let args: Vec<String> = args.collect();
    if !args.is_empty() {
        todo!("args not yet implemented {:?}", args)
    }

    let upbuild_file = upbuild_rs::find(".")?;

    let parsed_file = ClassicFile::parse_lines(
        std::fs::File::open(upbuild_file)
            .map(std::io::BufReader::new)?
            .lines()
            .map_while(std::result::Result::ok)
            .collect::<Vec<String>>() // TODO - had to collect into a vec to deal with map to &str
            .iter()
            .map(|x| x.as_str()))?;

    let exec = Exec::new(
        if cfg.print {
            upbuild_rs::print_runner()
        } else {
            upbuild_rs::process_runner()
        }
    );

    exec.run_with_tags(&parsed_file, &cfg.select)
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
