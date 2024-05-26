use std::collections::HashSet;

#[derive(Debug, PartialEq)]
pub struct Config {
    pub(crate) print: bool,
    pub(crate) triple: bool, // OLD_STYLE_ARGS_HANDLER
    pub(crate) select: HashSet<String>,
    pub(crate) reject: HashSet<String>,
    pub(crate) add: bool,
    pub(crate) argv0: String,
}

impl Config {
    pub fn print(&self) -> bool {
        self.print
    }

    pub fn add(&self) -> bool {
        self.add
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            print: false,
            triple: false,
            select: Default::default(),
            reject: Default::default(),
            add: false,
            argv0: String::from("upbuild"),
        }
    }
}

fn apply_tags(arg: &str, add: &mut HashSet<String> , drop: &mut HashSet<String>) -> bool {
    match arg.split_once('=') {
        Some((_, arg)) => {
            if !arg.is_empty() {
                add.insert(arg.to_string());
                drop.remove(arg);
                return true;
            }
        },
        None => return false,
    }
    false
}

/// Handles the `--ub-*` prefix command-line arguments and returns the
/// remaining command-line arguments to the caller.
impl Config {

    /// Parse the given parameters
    ///
    /// ```
    /// # use upbuild_rs::Config;
    /// let (args, cfg) = Config::parse(std::env::args());
    /// ```

    pub fn parse<T>(args: T) -> (std::iter::Peekable<T>, Config)
    where
        T: Iterator<Item=String>
    {
        let mut args = args.peekable();
        let mut cfg = Config { ..Default::default() };

        if let Some(arg) = args.next() {
            cfg.argv0 = arg;
        }

        while let Some(arg) = args.peek() {
            if let Some(s) = arg.strip_prefix("--") {
                match s {
                    "ub-print" => {
                        cfg.print = true;
                    },
                    "ub-add" => {
                        cfg.add = true;
                    },
                    "-" => { // ---
                        if super::OLD_STYLE_ARGS_HANDLER {
                            cfg.triple = true;  args.next(); break;
                        } else {
                            break; // Not handled
                        }
                    },
                    "" => { args.next(); break; },
                    _ => {
                        if arg.starts_with("--ub-select=") {
                            if ! apply_tags(arg, &mut cfg.select, &mut cfg.reject) {
                                break;
                            }
                        } else if arg.starts_with("--ub-reject=") {
                            if ! apply_tags(arg, &mut cfg.reject, &mut cfg.select) {
                                break;
                            }
                        } else {
                            break;
                        }
                    },
                };

            } else {
                break;
            }
            args.next();
        }
        (args, cfg)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn args<const N: usize>(args: [&str; N]) -> std::vec::IntoIter<String> {
        let v: Vec<String> =
            ["upbuild"].into_iter()
            .chain(args)
            .map(|x| x.to_string()).collect();
        v.into_iter()
    }

    fn do_parse<const N: usize>(a: [&str; N]) -> (Vec<String>, Config) {
        let (v, args) = Config::parse(args(a));
        (v.collect(), args)
    }

    #[test]
    fn test_parse() {
        let (v, args) = do_parse([]);
        assert!(v.is_empty(), "!is_empty: was {:?}", v);
        assert_eq!(args, Config::default());

        let (v, args) = do_parse(["a", "b"]);
        assert_eq!(v, ["a", "b"]);
        assert_eq!(args, Config::default());

        let (v, args) = do_parse(["--"]);
        assert!(v.is_empty(), "!is_empty: was {:?}", v);
        assert_eq!(args, Config::default());

        let (v, args) = do_parse(["--ub-print"]);
        assert!(v.is_empty(), "!is_empty: was {:?}", v);
        assert_eq!(args, Config { print: true, ..Config::default() });

        let (v, args) = do_parse(["--ub-print", "a", "b"]);
        assert_eq!(v, ["a", "b"]);
        assert_eq!(args, Config { print: true, ..Config::default() });

        // after any non-matched arguments we'accept normal arguments
        let (v, args) = do_parse(["a", "b", "--ub-print"]);
        assert_eq!(v, ["a", "b", "--ub-print"]);
        assert_eq!(args, Config { print: false, ..Config::default() });

        // check -- to end parsing
        let (v, args) = do_parse(["--", "--ub-print"]);
        assert_eq!(v, ["--ub-print"]);
        assert_eq!(args, Config { print: false, ..Config::default() });

        // check -- to end parsing
        let (v, args) = do_parse(["--"]);
        assert!(v.is_empty(), "!is_empty: was {:?}", v);
        assert_eq!(args, Config { ..Config::default() });
    }

    fn string_set<const N: usize>(list: [&str; N]) -> HashSet<String> {
        HashSet::from(list.map(|s| s.to_string()))
    }

    #[test]
    fn test_parse_tags() {
        let (v, args) = do_parse(["--ub-select=foo"]);
        assert!(v.is_empty(), "!is_empty: was {:?}", v);
        assert_eq!(args, Config { select: string_set(["foo"]), ..Config::default() });

        let (v, args) = do_parse(["--ub-reject=foo"]);
        assert!(v.is_empty(), "!is_empty: was {:?}", v);
        assert_eq!(args, Config { reject: string_set(["foo"]), ..Config::default() });

        let (v, args) = do_parse(["--ub-reject=foo", "--ub-select=bar"]);
        assert!(v.is_empty(), "!is_empty: was {:?}", v);
        assert_eq!(args, Config {
            select: string_set(["bar"]),
            reject: string_set(["foo"]),
            ..Config::default()
        });

        let (v, args) = do_parse(["--ub-reject=foo", "--ub-select=bar", "--ub-select=foo"]);
        assert!(v.is_empty(), "!is_empty: was {:?}", v);
        assert_eq!(args, Config {
            select: string_set(["bar", "foo"]),
            ..Config::default()
        });

        let (v, args) = do_parse(["--ub-reject=foo", "--ub-select=bar", "--ub-reject=bar"]);
        assert!(v.is_empty(), "!is_empty: was {:?}", v);
        assert_eq!(args, Config {
            reject: string_set(["bar", "foo"]),
            ..Config::default()
        });

        let (v, args) = do_parse(["--ub-reject=foo", "--ub-select=bar", "--", "--ub-reject=bar"]);
        assert_eq!(v, ["--ub-reject=bar"]);
        assert_eq!(args, Config {
            select: string_set(["bar"]),
            reject: string_set(["foo"]),
            ..Config::default()
        });

        let (v, args) = do_parse(["--ub-reject"]);
        assert_eq!(v, ["--ub-reject"]);
        assert_eq!(args, Config { ..Config::default() });

        let (v, args) = do_parse(["--ub-select"]);
        assert_eq!(v, ["--ub-select"]);
        assert_eq!(args, Config { ..Config::default() });

        let (v, args) = do_parse(["--ub-select="]);
        assert_eq!(v, ["--ub-select="]);
        assert_eq!(args, Config { ..Config::default() });
    }
}
