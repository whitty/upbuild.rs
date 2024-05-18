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
    Cd(String)
}

#[derive(Debug, Default)]
pub struct Cmd {
    args: Vec<String>,
    tags: HashSet<String>,
    cd: Option<String>,
    outfile: Option<String>,
    retmap: HashMap<RetCode, RetCode>,
    disabled: bool,
    manual: bool,
    recurse: bool,
}

impl Cmd {

    fn append_arg<T: Into<String>>(&mut self, arg: T) {
        self.args.push(arg.into());
    }

    fn new(exe: String) -> Cmd {
        let recurse = exe == "upbuild";
        let args = vec![exe];
        Cmd {
            args,
            recurse,
            ..Default::default()
        }
    }

    pub fn directory(self) -> PathBuf {
        let mut path = PathBuf::from(self.cd.unwrap_or(String::from(".")));
        if self.recurse {
            path.pop();
        }
        path
    }

    pub fn map_code(&self, c: RetCode) ->RetCode {
        *self.retmap.get(&c)
            .unwrap_or(&c)
    }

    pub fn args(&self) -> &Vec<String> {
        &self.args
    }
}

#[derive(Debug)]
pub struct ClassicFile {
    commands: Vec<Cmd>,
}

impl ClassicFile {
    pub fn len(&self) {
        self.commands.len();
    }
}

#[derive(Debug, PartialEq)]
enum Line {
    Flag(Flags),
    Arg(String),
    Comment,
    End
}

// Parse a single @retmap=entry
fn parse_retmap(def: &str) -> Result<HashMap<RetCode, RetCode>> {
    let mut h: HashMap<RetCode, RetCode> = HashMap::new();
    for entry in def.split(',') {
        let parts = entry.split_once("=>").ok_or(Error::InvalidRetMapDefinition(def.to_string()))?;
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

impl ClassicFile {

    pub fn parse_lines<'a, I>(lines: I) -> Result<ClassicFile>
    where I: Iterator<Item=&'a str>
    {
        let mut e: Option<Cmd> = None;
        let mut entries: Vec<Cmd> = Vec::new();

        for line in lines {
            let line = parse_line(line)?;

            match line {

                Line::Arg(f) => {
                    match e {
                        Some(ref mut cmd) => cmd.append_arg(f),
                        None => {
                            e.replace(Cmd::new(f));
                        },
                    }
                },

                Line::Flag(f) => {
                    match e {
                        Some(ref mut cmd) => {
                            // TODO detect duplicates
                            match f {
                                Flags::Disable => cmd.disabled = true,
                                Flags::Manual => cmd.manual = true,
                                Flags::Tags(tags) => cmd.tags = tags,
                                Flags::Outfile(filename) => cmd.outfile = Some(filename),
                                Flags::RetMap(map) => cmd.retmap = map,
                                Flags::Cd(dir) => cmd.cd = Some(dir),
                            }
                        },
                        None => { Err(Error::FlagBeforeCommand(format!("{:?}", f)))? },
                    }
                },

                Line::Comment => (), // Just drop it

                Line::End => {
                    match e {
                        Some(_) => entries.push(e.take().expect("isn't none")),
                        None => Err(Error::EmptyEntry)?,
                    }
                },
            }
        }

        match e {
            Some(_) => entries.push(e.take().expect("isn't none")),
            None => Err(Error::EmptyEntry)?,
        }

        Ok(ClassicFile{
            commands: entries,
        })
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

        assert_eq!(Line::Flag(Flags::Outfile("out.txt".into())), parse_line("@outfile=out.txt").expect("should succeed"));
        assert!(parse_retmap("@outfile=").is_err());
        assert!(parse_retmap("@outfile").is_err());

        assert_eq!(Line::Flag(Flags::Tags(string_set(["foo", "bar", "bat"]))), parse_line("@tags=foo,bar,bat").expect("should succeed"));
        assert_eq!(Line::Flag(Flags::Tags(HashSet::new())), parse_line("@tags=").expect("should succeed"));
        assert_eq!(Line::Flag(Flags::Tags(string_set(["foo", "bar=bat"]))), parse_line("@tags=foo,bar=bat").expect("should succeed"));
        assert!(parse_retmap("@tags").is_err());
    }

    fn parse(s: &str) -> ClassicFile {
        // basic test structure - printing in case of failure
        println!("'{}'", s);
        let file = ClassicFile::parse_lines(s.split_terminator("\n")).unwrap();
        println!("{:#?}", file);
        file
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
        assert_eq!(file.commands[0].outfile, None);
        assert_eq!(file.commands[0].args, vec!["make", "tests"]);

        assert_eq!(file.commands[1].tags, string_set(["target"]));
        assert!(!file.commands[1].disabled);
        assert!(!file.commands[1].manual);
        assert!(!file.commands[1].recurse);
        assert!(file.commands[1].retmap.is_empty());
        assert_eq!(file.commands[1].cd, None);
        assert_eq!(file.commands[1].outfile, None);
        assert_eq!(file.commands[1].args, vec!["make", "cross"]);

        assert_eq!(file.commands[2].tags, string_set(["release", "host"]));
        assert!(!file.commands[2].disabled);
        assert!(file.commands[2].manual);
        assert!(!file.commands[2].recurse);
        assert!(file.commands[2].retmap.is_empty());
        assert_eq!(file.commands[2].cd, None);
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
        assert_eq!(file.commands[0].outfile, None);
        assert_eq!(file.commands[0].args, vec!["make", "tests"]);

        assert!(file.commands[1].tags.is_empty());
        assert!(file.commands[1].disabled);
        assert!(!file.commands[1].manual);
        assert!(!file.commands[1].recurse);
        assert!(file.commands[1].retmap.is_empty());
        assert_eq!(file.commands[1].cd, None);
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
        assert_eq!(file.commands[0].outfile, None);
        assert_eq!(file.commands[0].args, vec!["make", "-j8"]);

        assert!(file.commands[1].tags.is_empty());
        assert!(!file.commands[1].disabled);
        assert!(!file.commands[1].manual);
        assert!(file.commands[1].recurse);
        assert!(file.commands[1].retmap.is_empty());
        assert_eq!(file.commands[1].cd, None);
        assert_eq!(file.commands[1].outfile, None);
        assert_eq!(file.commands[1].args, vec!["upbuild"]);
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
        assert_eq!(cmd.outfile, Some(String::from("log.txt")));
        assert_eq!(cmd.args, vec!["uv4", "-j0", "-b", "project.uvproj", "-o", "log.txt"]);

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
        assert_eq!(file.commands[0].outfile, None);
        assert_eq!(file.commands[0].args, vec!["make", "-j8"]);

        assert!(file.commands[1].tags.is_empty());
        assert!(!file.commands[1].disabled);
        assert!(!file.commands[1].manual);
        assert!(file.commands[1].recurse);
        assert!(file.commands[1].retmap.is_empty());
        assert_eq!(file.commands[1].cd, Some(String::from("/path/to/the/rest")));
        assert_eq!(file.commands[1].outfile, None);
        assert_eq!(file.commands[1].args, vec!["upbuild"]);
    }

}
