// SPDX-License-Identifier: GPL-3.0-or-later
// (C) Copyright 2024-2025 Greg Whiteley

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::{Error, Result};
use super::exec::RetCode;

#[derive(Debug, PartialEq)]
enum Flags {
    Disable,
    Tags(HashSet<String>),
    Manual,
    Outfile(String),
    RetMap(HashMap<RetCode, RetCode>),
    Cd(String),
    Mkdir(String),
}

#[derive(Debug, Default)]
pub struct Cmd {
    args: Vec<String>,
    tags: HashSet<String>,
    cd: Option<String>,
    mkdir: Option<String>,
    outfile: Option<String>,
    retmap: HashMap<RetCode, RetCode>,
    disabled: bool,
    manual: bool,
    recurse: bool,
    dotenvs: Vec<String>,
}

#[derive(Debug, PartialEq)]
enum HeaderFlags {
    Env(String),
}

#[derive(Debug, Default)]
pub struct Header {
    dotenvs: Vec<String>,
}

impl Cmd {

    fn append_arg<T: Into<String>>(&mut self, arg: T) {
        self.args.push(arg.into());
    }

    fn append_dotenv<T: Into<String>>(&mut self, arg: T) {
        self.dotenvs.push(arg.into());
    }

    fn new<T: Into<String>>(exe: T) -> Cmd {
        let exe = exe.into();
        let recurse = exe == "upbuild";
        let args = vec![exe];
        Cmd {
            args,
            recurse,
            ..Default::default()
        }
    }

    pub fn out_file(&self) -> Option<PathBuf> {
        self.outfile.as_ref().map(|ref f| PathBuf::from(f))
    }

    pub fn recurse(&self) -> bool {
        self.recurse
    }

    pub fn directory(&self) -> Option<PathBuf> {
        match self.cd {
            Some(ref d) => Some(PathBuf::from(d)),
            None => {
                if self.recurse {
                    return Some(PathBuf::from(".."));
                }
                None
            },
        }
    }

    pub fn mk_dir(&self) -> Option<PathBuf> {
        self.mkdir.as_ref().map(PathBuf::from)
    }

    pub fn map_code(&self, c: RetCode) ->RetCode {
        *self.retmap.get(&c)
            .unwrap_or(&c)
    }

    pub fn args(&self) -> &[String]  {
        self.args.as_ref()
    }

    pub fn enabled_with_reject(&self, select_tags: &HashSet<String>, reject_tags: &HashSet<String>) -> bool {
        if self.disabled {
            return false;
        }

        // reject if matched
        if !reject_tags.is_disjoint(&self.tags) {
            return false;
        }

        let no_tags = select_tags.is_empty();
        if self.manual &&
            (no_tags || select_tags.is_disjoint(&self.tags)) {
            return false;
        }

        if ! no_tags {
            // There are some tags - must match
            return !select_tags.is_disjoint(&self.tags);
        }
        true
    }

    pub fn dotenv(&self) -> &[String]  {
        self.dotenvs.as_ref()
    }
}

/// Read an `.upbuild` file in the "classic" "simple" format
#[derive(Debug)]
pub struct ClassicFile {
    pub(crate) commands: Vec<Cmd>, // TODO - pub(crate) is lazy)
    pub(crate) header: Header,
}

#[derive(Debug, PartialEq)]
enum Line {
    Flag(Flags),
    Arg(String),
    HeaderFlag(HeaderFlags),
    HeaderSeparator,
    Comment,
    End
}

impl Header {
    fn append_dotenv<T: Into<String>>(&mut self, arg: T) {
        self.dotenvs.push(arg.into());
    }

    fn new() -> Header {
        Header {
            ..Default::default()
        }
    }

    pub fn dotenv(&self) -> &[String]  {
        self.dotenvs.as_ref()
    }
}

// Parse a single @retmap=entry
fn parse_retmap(def: &str) -> Result<HashMap<RetCode, RetCode>> {
    let mut h: HashMap<RetCode, RetCode> = HashMap::new();
    for entry in def.split(',') {
        let parts = entry.split_once("=>").ok_or_else(|| Error::InvalidRetMapDefinition(def.to_string()))?;
        let a = str::parse::<RetCode>(parts.0).map_err(|_| Error::InvalidRetMapDefinition(parts.0.to_string()))?;
        let b = str::parse::<RetCode>(parts.1).map_err(|_| Error::InvalidRetMapDefinition(parts.1.to_string()))?;
        h.insert(a, b);
    }
    Ok(h)
}

fn parse_line(l: &str) -> Result<Line> {
    match l {
        "@disable" => Ok(Line::Flag(Flags::Disable)),
        "@manual" => Ok(Line::Flag(Flags::Manual)),
        "&&" => Ok(Line::End),
        s if s.starts_with("@---") => Ok(Line::HeaderSeparator),
        _ => {
            if l.starts_with('#') {
                Ok(Line::Comment)
            } else if l.starts_with('@') {
                match split_flag(l)? {
                    ("tags", tags) => Ok(Line::Flag(Flags::Tags(
                        if tags.is_empty() { // explicitly don't split ""
                            HashSet::new()
                        } else {
                            tags.split(',')
                                .map(|x| x.to_string())
                                .collect()
                        }
                    ))),
                    ("retmap", map) => Ok(Line::Flag(Flags::RetMap(parse_retmap(map)?))),
                    ("outfile", outfile) => Ok(Line::Flag(Flags::Outfile(outfile.to_string()))),
                    ("cd", dir) => Ok(Line::Flag(Flags::Cd(dir.to_string()))),
                    ("mkdir", dir) => Ok(Line::Flag(Flags::Mkdir(dir.to_string()))),
                    ("env", f) => Ok(Line::HeaderFlag(HeaderFlags::Env(f.to_string()))),
                    ("disable", "") => Ok(Line::Flag(Flags::Disable)),
                    ("manual", "") => Ok(Line::Flag(Flags::Manual)),
                    (&_, _) => Err(Error::InvalidTag(l.to_string()))
                }
            } else {
                Ok(Line::Arg(l.to_string()))
            }
        }
    }
}

fn split_flag(l: &str) -> Result<(&str, &str)> {
    if let Some(rest) = l.strip_prefix('@') {
        return Ok(rest.split_once('=').unwrap_or((rest, "")));
    }
    Err(Error::InvalidTag(l.to_string()))
}

enum HeaderDetectState {
    InHeader,
    InBody,
    Unknown // We don't know yet
}

impl ClassicFile {

    /// Create a [ClassicFile] from the given iterator providing lines
    pub fn parse_lines<I, T>(lines: I) -> Result<ClassicFile>
    where
        I: Iterator<Item=T>,
        T: std::borrow::Borrow<str>
    {
        let mut entry: Option<Cmd> = None;
        let mut entries: Vec<Cmd> = Vec::new();
        let mut header_state = HeaderDetectState::Unknown;
        let mut header = Header::new();

        for line in lines {
            let line = parse_line(line.borrow())?;

            match line {
                Line::HeaderSeparator => {
                    match header_state {
                        HeaderDetectState::InHeader | HeaderDetectState::Unknown => header_state = HeaderDetectState::InBody,
                        HeaderDetectState::InBody => { Err(Error::InvalidHeaderField(String::from("Header separator not allowed here")))? },
                    }
                }

                Line::Arg(f) => {
                    header_state = HeaderDetectState::InBody;
                    match entry {
                        Some(ref mut cmd) => cmd.append_arg(f),
                        None => {
                            entry.replace(Cmd::new(f));
                        },
                    }
                },

                Line::Flag(f) => {
                    header_state = HeaderDetectState::InBody;
                    match entry {
                        Some(ref mut cmd) => {
                            // TODO detect duplicates
                            match f {
                                Flags::Disable => cmd.disabled = true,
                                Flags::Manual => cmd.manual = true,
                                Flags::Tags(tags) => cmd.tags = tags,
                                Flags::Outfile(filename) => cmd.outfile = Some(filename),
                                Flags::RetMap(map) => cmd.retmap = map,
                                Flags::Cd(dir) => cmd.cd = Some(dir),
                                Flags::Mkdir(dir) => cmd.mkdir = Some(dir),
                            }
                        },
                        None => { Err(Error::FlagBeforeCommand(format!("{:?}", f)))? },
                    }
                },

                Line::Comment => (), // Just drop it

                Line::End => {
                    header_state = HeaderDetectState::InBody;
                    match entry {
                        Some(_) => entries.push(entry.take().expect("isn't none")),
                        None => Err(Error::EmptyEntry)?,
                    }
                },

                Line::HeaderFlag(header_flags) => {
                    match header_state {
                        HeaderDetectState::InHeader => (),
                        HeaderDetectState::InBody => {

                            #[allow(irrefutable_let_patterns)]
                            if let HeaderFlags::Env(e) = &header_flags {
                                // Special case env is
                                match entry {
                                    Some(ref mut cmd) => {
                                        cmd.append_dotenv(e);
                                        continue; // avoid further header processing (avoid the borrow checker at least)
                                    }
                                    None => { Err(Error::FlagBeforeCommand(format!("@env={}", e)))? },
                                }
                            } else {
                                Err(Error::InvalidHeaderField(String::from("Header field not allowed here")))?;
                            }
                        },
                        HeaderDetectState::Unknown => header_state = HeaderDetectState::InHeader,
                    }

                    match header_flags {
                        HeaderFlags::Env(e) => {
                            header.append_dotenv(e);
                        }
                    }
                },
            }
        }

        match entry {
            Some(_) => entries.push(entry.take().expect("isn't none")),
            None => Err(Error::EmptyEntry)?,
        }

        Ok(ClassicFile{
            header,
            commands: entries,
        })
    }

    /// Implement --ub-add, adding the provided_args to the .upbuild file
    /// at the given path - creating if if required.
    pub fn add<I, T>(provided_args: I, path: PathBuf) -> Result<()>
    where
        I: Iterator<Item=T>,
        T: std::borrow::Borrow<str>
    {
        use std::io::{Seek, Write, SeekFrom};

        let args_str = provided_args
            .fold(String::new(), |s, x| s + x.borrow() + "\n");

        if !args_str.is_empty() {

            let mut f = std::fs::File::options()
                .create(true)
                .truncate(false)
                .write(true).open(path)?;

            let pos = f.seek(SeekFrom::End(0))?;

            if pos != 0 {
                f.write_all("&&\n".as_bytes())?;
            }
            f.write_all(args_str.as_bytes())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_split_flag() {
        assert_eq!(("retmap", "1=>0"), split_flag("@retmap=1=>0").expect("should succeed"));
        assert_eq!(("disable", ""), split_flag("@disable").expect("should succeed"));
        assert!(split_flag("foo").is_err());
        assert!(split_flag("").is_err());
    }

    #[test]
    fn test_parse_retmap() {
        assert_eq!(HashMap::from([(1, 0)]), parse_retmap("1=>0").expect("should succeed"));
        assert_eq!(HashMap::from([(1, 0),
                                  (0, 1),
                                  (200000, 200001)]),
                   parse_retmap("1=>0,0=>1,200000=>200001").expect("should succeed"));
        assert!(parse_retmap("").is_err());
        assert!(parse_retmap("foo").is_err());
        assert!(parse_retmap("1=>0,bar").is_err());
        assert!(parse_retmap("1=>0,0").is_err());
    }

    fn string_set<const N: usize>(list: [&str; N]) -> HashSet<String> {
        HashSet::from(list.map(|s| s.to_string()))
    }

    #[test]
    fn test_parse_line_flags() {
        assert_eq!(Line::Flag(Flags::Disable), parse_line("@disable").expect("should succeed"));
        assert!(parse_retmap("@disable=").is_err());
        assert!(parse_retmap("@disabl").is_err());

        assert_eq!(Line::Flag(Flags::Manual), parse_line("@manual").expect("should succeed"));
        assert!(parse_retmap("@manual=").is_err());
        assert!(parse_retmap("@manual").is_err());

        assert_eq!(Line::Flag(Flags::RetMap(HashMap::from([(1, 0), (0, 1)]))),
                   parse_line("@retmap=0=>1,1=>0").expect("should succeed"));
        assert!(parse_retmap("@retmap=0=>1,").is_err());
        assert!(parse_retmap("@retmap").is_err());

        assert_eq!(Line::Flag(Flags::Cd("/path/to".into())), parse_line("@cd=/path/to").expect("should succeed"));
        assert!(parse_retmap("@cd=").is_err());
        assert!(parse_retmap("@cd").is_err());

        assert_eq!(Line::Flag(Flags::Mkdir("/path/to".into())), parse_line("@mkdir=/path/to").expect("should succeed"));
        assert!(parse_retmap("@mkdir=").is_err());
        assert!(parse_retmap("@mkdir").is_err());

        assert_eq!(Line::Flag(Flags::Outfile("out.txt".into())), parse_line("@outfile=out.txt").expect("should succeed"));
        assert!(parse_retmap("@outfile=").is_err());
        assert!(parse_retmap("@outfile").is_err());

        assert_eq!(Line::Flag(Flags::Tags(string_set(["foo", "bar", "bat"]))), parse_line("@tags=foo,bar,bat").expect("should succeed"));
        assert_eq!(Line::Flag(Flags::Tags(HashSet::new())), parse_line("@tags=").expect("should succeed"));
        assert_eq!(Line::Flag(Flags::Tags(string_set(["foo", "bar=bat"]))), parse_line("@tags=foo,bar=bat").expect("should succeed"));
        assert!(parse_retmap("@tags").is_err());
    }

    fn parse_(s: &str) -> Result<ClassicFile>  {
        // basic test structure - printing in case of failure
        println!("'{}'", s);
        let file = ClassicFile::parse_lines(s.lines());
        println!("{:#?}", file);
        file
    }

    fn parse(s: &str) -> ClassicFile {
        parse_(s).unwrap()
    }

    fn expect_error(s: &str) -> Error {
        parse_(s).unwrap_err()
    }

    #[test]
    fn test_tags_parsing() {

        let s = r"make
@tags=host
tests
&&
make
@tags=target
cross
&&
make
@manual
@tags=release,host
install
";
        let file = parse(s);

        assert_eq!(3, file.commands.len());
        assert_eq!(file.commands[0].tags, string_set(["host"]));
        assert!(!file.commands[0].disabled);
        assert!(!file.commands[0].manual);
        assert!(!file.commands[0].recurse);
        assert!(file.commands[0].retmap.is_empty());
        assert_eq!(file.commands[0].cd, None);
        assert_eq!(file.commands[0].mkdir, None);
        assert_eq!(file.commands[0].outfile, None);
        assert_eq!(file.commands[0].args, vec!["make", "tests"]);

        assert_eq!(file.commands[1].tags, string_set(["target"]));
        assert!(!file.commands[1].disabled);
        assert!(!file.commands[1].manual);
        assert!(!file.commands[1].recurse);
        assert!(file.commands[1].retmap.is_empty());
        assert_eq!(file.commands[1].cd, None);
        assert_eq!(file.commands[1].mkdir, None);
        assert_eq!(file.commands[1].outfile, None);
        assert_eq!(file.commands[1].args, vec!["make", "cross"]);

        assert_eq!(file.commands[2].tags, string_set(["release", "host"]));
        assert!(!file.commands[2].disabled);
        assert!(file.commands[2].manual);
        assert!(!file.commands[2].recurse);
        assert!(file.commands[2].retmap.is_empty());
        assert_eq!(file.commands[2].cd, None);
        assert_eq!(file.commands[2].mkdir, None);
        assert_eq!(file.commands[2].outfile, None);
        assert_eq!(file.commands[2].args, vec!["make", "install"]);
    }

    #[test]
    fn test_disable() {

        let s = r"make
tests
&&
make
@disable
install
";
        let file = parse(s);
        assert_eq!(2, file.commands.len());

        assert!(file.commands[0].tags.is_empty());
        assert!(!file.commands[0].disabled);
        assert!(!file.commands[0].manual);
        assert!(!file.commands[0].recurse);
        assert!(file.commands[0].retmap.is_empty());
        assert_eq!(file.commands[0].cd, None);
        assert_eq!(file.commands[0].mkdir, None);
        assert_eq!(file.commands[0].outfile, None);
        assert_eq!(file.commands[0].args, vec!["make", "tests"]);

        assert!(file.commands[1].tags.is_empty());
        assert!(file.commands[1].disabled);
        assert!(!file.commands[1].manual);
        assert!(!file.commands[1].recurse);
        assert!(file.commands[1].retmap.is_empty());
        assert_eq!(file.commands[1].cd, None);
        assert_eq!(file.commands[1].mkdir, None);
        assert_eq!(file.commands[1].outfile, None);
        assert_eq!(file.commands[1].args, vec!["make", "install"]);
    }

    #[test]
    fn test_recursive() {

        let s = r"make
-j8
&&
upbuild
";
        let file = parse(s);
        assert_eq!(2, file.commands.len());

        assert!(file.commands[0].tags.is_empty());
        assert!(!file.commands[0].disabled);
        assert!(!file.commands[0].manual);
        assert!(!file.commands[0].recurse);
        assert!(file.commands[0].retmap.is_empty());
        assert_eq!(file.commands[0].cd, None);
        assert_eq!(file.commands[0].mkdir, None);
        assert_eq!(file.commands[0].outfile, None);
        assert_eq!(file.commands[0].args, vec!["make", "-j8"]);
        assert_eq!(file.commands[0].directory(), None);

        assert!(file.commands[1].tags.is_empty());
        assert!(!file.commands[1].disabled);
        assert!(!file.commands[1].manual);
        assert!(file.commands[1].recurse);
        assert!(file.commands[1].retmap.is_empty());
        assert_eq!(file.commands[1].cd, None);
        assert_eq!(file.commands[1].mkdir, None);
        assert_eq!(file.commands[1].outfile, None);
        assert_eq!(file.commands[1].args, vec!["upbuild"]);
        assert_eq!(file.commands[1].directory().expect("should exist"), std::path::Path::new(".."));
    }

    #[test]
    fn test_retmap() {

        let s = r"uv4
# uv4 returns 1 if errors occurred - our library includes
# suck so map 1 to a success
@retmap=1=>0
# Also sucks as it outputs to a file
@outfile=log.txt
-j0
-b
project.uvproj
-o
log.txt
";

        let file = parse(s);
        assert_eq!(1, file.commands.len());
        let cmd = &file.commands[0];

        assert!(cmd.tags.is_empty());
        assert!(!cmd.disabled);
        assert!(!cmd.manual);
        assert!(!cmd.recurse);
        assert_eq!(cmd.retmap, HashMap::from([(1, 0)]));
        assert_eq!(cmd.cd, None);
        assert_eq!(cmd.mkdir, None);
        assert_eq!(cmd.outfile, Some(String::from("log.txt")));
        assert_eq!(cmd.args, vec!["uv4", "-j0", "-b", "project.uvproj", "-o", "log.txt"]);
        assert_eq!(cmd.out_file(), Some(PathBuf::from("log.txt")));

        for (v, exp) in [
            (0,0),
            (1,0),
            (2,2),
            (-1,-1),
            (10000,10000),
            (-10000,-10000),
        ] {
            assert_eq!(cmd.map_code(v), exp, "Mapping {} expected {}", v, exp);
        }
    }

    #[test]
    fn test_cd_recursive() {

        let s = r"make
-j8
&&
upbuild
@cd=/path/to/the/rest
";
        let file = parse(s);
        assert_eq!(2, file.commands.len());

        assert!(file.commands[0].tags.is_empty());
        assert!(!file.commands[0].disabled);
        assert!(!file.commands[0].manual);
        assert!(!file.commands[0].recurse);
        assert!(file.commands[0].retmap.is_empty());
        assert_eq!(file.commands[0].cd, None);
        assert_eq!(file.commands[0].mkdir, None);
        assert_eq!(file.commands[0].outfile, None);
        assert_eq!(file.commands[0].args, vec!["make", "-j8"]);
        assert_eq!(file.commands[0].directory(), None);

        assert!(file.commands[1].tags.is_empty());
        assert!(!file.commands[1].disabled);
        assert!(!file.commands[1].manual);
        assert!(file.commands[1].recurse);
        assert!(file.commands[1].retmap.is_empty());
        assert_eq!(file.commands[1].cd, Some(String::from("/path/to/the/rest")));
        assert_eq!(file.commands[1].mkdir, None);
        assert_eq!(file.commands[1].outfile, None);
        assert_eq!(file.commands[1].args, vec!["upbuild"]);
        assert_eq!(file.commands[1].directory().expect("should exist"), std::path::Path::new("/path/to/the/rest"));
    }

    fn check_select_tags<const N: usize>(file: &ClassicFile, select_tags: HashSet<String>, expected: [bool; N]) {
        println!("Expecting {:?} tags to result in {:?}", select_tags, expected);
        assert!(file.commands.iter()
                .map(|x| x.enabled_with_reject(&select_tags, &HashSet::new()))
                .eq(expected.into_iter()));
    }

    fn check_select_reject_tags<const N: usize>(file: &ClassicFile, select_tags: HashSet<String>,
                                                reject_tags: HashSet<String>, expected: [bool; N]) {
        println!("Expecting select={:?} reject={:?} tags to result in {:?}", select_tags, reject_tags, expected);
        assert!(file.commands.iter()
                .map(|x| x.enabled_with_reject(&select_tags, &reject_tags))
                .eq(expected.into_iter()));
    }

    #[test]
    fn test_tags_selection() {

        let s = r"make
@tags=host
tests
&&
make
@tags=target
cross
&&
make
@manual
@tags=release,host
install
";
        let file = parse(s);

        assert_eq!(3, file.commands.len());
        assert_eq!(file.commands[0].tags, string_set(["host"]));
        assert!(!file.commands[0].disabled);
        assert!(!file.commands[0].manual);

        assert_eq!(file.commands[1].tags, string_set(["target"]));
        assert!(!file.commands[1].disabled);
        assert!(!file.commands[1].manual);

        assert_eq!(file.commands[2].tags, string_set(["release", "host"]));
        assert!(!file.commands[2].disabled);
        assert!(file.commands[2].manual);
        assert!(!file.commands[2].recurse);

        check_select_tags(&file, string_set([]), [true, true, false]);
        check_select_tags(&file, string_set(["host"]), [true, false, true]);
        check_select_tags(&file, string_set(["release"]), [false, false, true]);
        check_select_tags(&file, string_set(["target"]), [false, true, false]);
        check_select_tags(&file, string_set(["release", "host"]), [true, false, true]);
        check_select_tags(&file, string_set(["release", "target"]), [false, true, true]);
        check_select_tags(&file, string_set(["release", "target", "host"]), [true, true, true]);

        check_select_reject_tags(&file,
                                 string_set(["release", "target", "host"]),
                                 string_set([]), [true, true, true]);
        check_select_reject_tags(&file,
                                 string_set([]),
                                 string_set([]), [true, true, false]);
        check_select_reject_tags(&file,
                                 string_set([]),
                                 string_set(["target"]), [true, false, false]);
        check_select_reject_tags(&file,
                                 string_set([]),
                                 string_set(["host"]), [false, true, false]);
        check_select_reject_tags(&file,
                                 string_set(["release"]),
                                 string_set(["host"]), [false, false, false]);
        check_select_reject_tags(&file,
                                 string_set(["release", "target"]),
                                 string_set(["host"]), [false, true, false]);
        check_select_reject_tags(&file,
                                 string_set(["host"]),
                                 string_set(["release"]), [true, false, false]);
    }

    #[test]
    fn test_cd_mkdir() {

        let s = r"cmake
@cd=build
@mkdir=build
..
--fresh
&&
cmake
@cd=build
--build
.
";
        let file = parse(s);
        assert_eq!(2, file.commands.len());

        assert!(file.commands[0].tags.is_empty());
        assert!(!file.commands[0].disabled);
        assert!(!file.commands[0].manual);
        assert!(!file.commands[0].recurse);
        assert!(file.commands[0].retmap.is_empty());
        assert_eq!(file.commands[0].cd, Some(String::from("build")));
        assert_eq!(file.commands[0].mkdir, Some(String::from("build")));
        assert_eq!(file.commands[0].outfile, None);
        assert_eq!(file.commands[0].args, vec!["cmake", "..", "--fresh"]);
        assert_eq!(file.commands[0].directory().expect("should exist"), std::path::Path::new("build"));

        assert!(file.commands[1].tags.is_empty());
        assert!(!file.commands[1].disabled);
        assert!(!file.commands[1].manual);
        assert!(!file.commands[1].recurse);
        assert!(file.commands[1].retmap.is_empty());
        assert_eq!(file.commands[1].cd, Some(String::from("build")));
        assert_eq!(file.commands[1].mkdir, None);
        assert_eq!(file.commands[1].outfile, None);
        assert_eq!(file.commands[1].args, vec!["cmake", "--build", "."]);
        assert_eq!(file.commands[1].directory().expect("should exist"), std::path::Path::new("build"));
    }

    #[test]
    fn test_header_basic_parse() {

        let s = r"@env=.env
@---
make
tests
&&
make
@disable
install
";
        let file = parse(s);
        assert_eq!(1, file.header.dotenvs.len());
        assert_eq!(file.header.dotenvs[0], ".env");

        assert_eq!(2, file.commands.len());

        assert!(file.commands[0].tags.is_empty());
        assert!(!file.commands[0].disabled);
        assert!(!file.commands[0].manual);
        assert!(!file.commands[0].recurse);
        assert!(file.commands[0].retmap.is_empty());
        assert_eq!(file.commands[0].cd, None);
        assert_eq!(file.commands[0].mkdir, None);
        assert_eq!(file.commands[0].outfile, None);
        assert_eq!(file.commands[0].args, vec!["make", "tests"]);

        assert!(file.commands[1].tags.is_empty());
        assert!(file.commands[1].disabled);
        assert!(!file.commands[1].manual);
        assert!(!file.commands[1].recurse);
        assert!(file.commands[1].retmap.is_empty());
        assert_eq!(file.commands[1].cd, None);
        assert_eq!(file.commands[1].mkdir, None);
        assert_eq!(file.commands[1].outfile, None);
        assert_eq!(file.commands[1].args, vec!["make", "install"]);

        let s = r"@env=.env
@---
@---
make
tests
&&
make
@disable
install
";
        let e = expect_error(s);
        assert!(e.to_string().contains("Header separator not allowed"), "e={}", e);

        let s = r"@env=.env
@---
make
@env=.env2
tests
&&
make
@disable
install
";
        let file = parse(s);
        assert_eq!(1, file.header.dotenvs.len());
        assert_eq!(file.header.dotenvs[0], ".env");

        assert_eq!(2, file.commands.len());

        assert!(file.commands[0].tags.is_empty());
        assert!(!file.commands[0].disabled);
        assert!(!file.commands[0].manual);
        assert!(!file.commands[0].recurse);
        assert!(file.commands[0].retmap.is_empty());
        assert_eq!(file.commands[0].cd, None);
        assert_eq!(file.commands[0].mkdir, None);
        assert_eq!(file.commands[0].outfile, None);
        assert_eq!(file.commands[0].args, vec!["make", "tests"]);
        assert_eq!(1, file.commands[0].dotenvs.len());
        assert_eq!(file.commands[0].dotenvs[0], ".env2");

        assert!(file.commands[1].tags.is_empty());
        assert!(file.commands[1].disabled);
        assert!(!file.commands[1].manual);
        assert!(!file.commands[1].recurse);
        assert!(file.commands[1].retmap.is_empty());
        assert_eq!(file.commands[1].cd, None);
        assert_eq!(file.commands[1].mkdir, None);
        assert_eq!(file.commands[1].outfile, None);
        assert_eq!(file.commands[1].args, vec!["make", "install"]);
        assert_eq!(0, file.commands[1].dotenvs.len());

        let s = r"make
@env=.env
tests
&&
make
@disable
install
";
        let file = parse(s);
        assert_eq!(0, file.header.dotenvs.len());

        assert_eq!(2, file.commands.len());

        assert!(file.commands[0].tags.is_empty());
        assert!(!file.commands[0].disabled);
        assert!(!file.commands[0].manual);
        assert!(!file.commands[0].recurse);
        assert!(file.commands[0].retmap.is_empty());
        assert_eq!(file.commands[0].cd, None);
        assert_eq!(file.commands[0].mkdir, None);
        assert_eq!(file.commands[0].outfile, None);
        assert_eq!(file.commands[0].args, vec!["make", "tests"]);
        assert_eq!(1, file.commands[0].dotenvs.len());
        assert_eq!(file.commands[0].dotenvs[0], ".env");

        assert!(file.commands[1].tags.is_empty());
        assert!(file.commands[1].disabled);
        assert!(!file.commands[1].manual);
        assert!(!file.commands[1].recurse);
        assert!(file.commands[1].retmap.is_empty());
        assert_eq!(file.commands[1].cd, None);
        assert_eq!(file.commands[1].mkdir, None);
        assert_eq!(file.commands[1].outfile, None);
        assert_eq!(file.commands[1].args, vec!["make", "install"]);
        assert_eq!(0, file.commands[1].dotenvs.len());
    }

}
