use std::collections::{HashMap, HashSet};

use super::{Error, Result};

#[derive(Debug)]
enum Flags {
    Disable,
    Tags(HashSet<String>),
    Manual,
    Outfile(String),
    RetMap(HashMap<isize, isize>),
    Cd(String)
}

#[derive(Debug)]
pub struct Cmd {
    args: Vec<String>,
    tags: HashSet<String>,
    cd: Option<String>,
    outfile: Option<String>,
    retmap: HashMap<isize, isize>,
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
        return Cmd {
            args: args,
            tags: HashSet::new(),
            cd: None,
            manual: false,
            recurse: recurse,
            outfile: None,
            retmap: HashMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct ClassicFile {
    commands: Vec<Cmd>,
}

#[derive(Debug)]
enum Line {
    Flag(Flags),
    Arg(String),
    Comment,
    End
}

// Parse a single @retmap=entry
fn parse_retmap(def: &str) -> Result<HashMap<isize, isize>> {
    let mut h: HashMap<isize, isize> = HashMap::new();
    for entry in def.split(',') {
        let parts = entry.split_once("=>").ok_or(Error::InvalidRetMapDefinition(def.to_string()))?;
        let a = str::parse::<isize>(parts.0).map_err(|_| Error::InvalidRetMapDefinition(parts.0.to_string()))?;
        let b = str::parse::<isize>(parts.1).map_err(|_| Error::InvalidRetMapDefinition(parts.1.to_string()))?;
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
                    ("tags", tags) => Ok(Line::Flag(Flags::Tags(tags.split(',')
                                                                .map(|x| x.to_string())
                                                                .collect()))),
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

fn split_flag<'a>(l: &'a str) -> Result<(&'a str, &'a str)> {
    if l.starts_with('@') {
        let l = &l[1..];
        return Ok(l.split_once('=').unwrap_or((l, "")));
    }
    Err(Error::InvalidTag(l.to_string()))
}

impl ClassicFile {

    pub fn parse_iter<'a, I>(lines: I) -> Result<ClassicFile>
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
                                Flags::Disable => (), // Just drop it
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

        return Ok(ClassicFile{
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
        println!("'{}'", s);
        let file = ClassicFile::parse_iter(s.split("\n")).unwrap();
        println!("{:#?}", file);
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
        ClassicFile::parse_iter(s.split("\n")).unwrap();
    }

    #[test]
    fn test_recursive() {

        let s = r"make
-j8
&&
upbuild
";
        ClassicFile::parse_iter(s.split("\n")).unwrap();
    }

    #[test]
    fn test_cd_recursive() {

        let s = r"make
-j8
&&
upbuild
@cd=/path/to/the/rest
";
        ClassicFile::parse_iter(s.split("\n")).unwrap();
    }

}
