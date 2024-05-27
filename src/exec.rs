// SPDX-License-Identifier: GPL-3.0-or-later
// (C) Copyright 2024 Greg Whiteley

use super::{Error, Result, Config};
use super::file::ClassicFile;

use std::path::{Path, PathBuf};
use std::process::Command;

pub type RetCode = isize;

/// Create a normal runner for [`Exec`] that actually runs the commands
pub fn process_runner() -> Box<dyn Runner> {
   Box::new(ProcessRunner {})
}

/// Create a runner for [`Exec`] that just prints the commands
pub fn print_runner() -> Box<dyn Runner> {
   Box::new(PrintRunner {})
}

/// The Exec struct implements the actual iteration through the
/// `.upbuild` file and dispatch of the derived commands after
/// applying arguments and tags.
pub struct Exec {
    runner: Box<dyn Runner>,
}

pub trait Runner {
    /// Run a given command in the provided directory
    fn run(&self, cmd: Vec<String>, cd: &Option<PathBuf>) -> Result<RetCode>;

    /// Display output from a file defined by @outfile
    fn display_output(&self, file: &Path) -> Result<()>;

    /// Output additional data
    fn display(&self, s: &str);
}

impl Exec {

    /// Create a new executor with the given Runner as environment
    pub fn new(runner: Box<dyn Runner>) -> Self {
        Self { runner }
    }

    fn relative_dir(path: &Path) -> Option<PathBuf> {
        if let Some(parent) = path.parent() {
            if parent == Path::new(".") || parent == Path::new("") {
                return None;
            }
            return Some(parent.into())
        }
        None
    }

    // Show entering message
    fn show_entering(&self, working_dir: &Option<PathBuf>) {
        if let Some(ref d) = working_dir {
            let dd = d.canonicalize(); // full path
            let dir = dd.as_ref().unwrap_or(d); // or fallback to d
            self.runner.display(format!("upbuild: Entering directory `{}'", dir.display()).as_str());
        }
    }

    fn show_entering_always(&self, working_dir: &Option<PathBuf>) {
        if working_dir.is_none() {
            let dot = Some(PathBuf::from("."));
            return self.show_entering(&dot);
        }
        self.show_entering(working_dir)
    }

    /// Run the given classic file, args, and config
    pub fn run(&self, path: &Path, file: &ClassicFile, cfg: &Config, provided_args: &[String]) -> Result<()> {
        let main_working_dir = Exec::relative_dir(path);
        self.show_entering(&main_working_dir);

        let mut last_dir = main_working_dir.clone(); // TODO clones

        let argv0 = &cfg.argv0;
        for cmd in &file.commands {
            if ! cmd.enabled_with_reject(&cfg.select, &cfg.reject) {
                continue;
            }
            let args = Self::with_args(cmd.args(), provided_args,
                                       if cmd.recurse() {
                                           Some(argv0)
                                       } else {
                                           None
                                       }, cfg.triple // OLD_STYLE_ARGS_HANDLER
            );

            let cmd_dir = cmd.directory();
            let run_dir = if cmd_dir.is_some() {
                &cmd_dir
            } else {
                &main_working_dir
            };

            if run_dir != &last_dir {
                self.show_entering_always(run_dir); // after initial cd always show any change
                last_dir = run_dir.clone(); // TODO clones
            }

            let code = self.runner.run(args, run_dir)?;
            let c = cmd.map_code(code);
            if c != 0 {
                return Err(Error::ExitWithExitCode(c));
            }

            if let Some(outfile) = cmd.out_file() {
                self.runner.display_output(outfile.as_path())?;
            }
        }

        Ok(())
    }

    fn with_args(args: std::slice::Iter<'_, String>, provided_args: &[String], argv0: Option<&String>, triple: bool) -> Vec<String> {
        // map helper for selecting argv[0] from args or argv0
        let mut replace_first = argv0.is_some();
        let replace_argv0 = |x| {
            if replace_first {
                replace_first = false;
                argv0.unwrap()
            } else {
                x
            }
        };

        if provided_args.is_empty() && !(crate::OLD_STYLE_ARGS_HANDLER && triple) {

            let mut first_separator = true;
            return args
                .map(replace_argv0)
                .filter(|x| {
                    if first_separator && x == &"--" {
                        first_separator = false;
                        return false;
                    }
                    true
                })
                .map(String::from)
                .collect();
        }

        if super::OLD_STYLE_ARGS_HANDLER {

            // I'm just going to hack this in to get the tests passing then back it out
            let mut has_dash_dash = false;
            let result = args.take_while(|x| {
                if x != &"--" {
                    return true
                }
                has_dash_dash = true;
                false
            })
                .map(replace_argv0)
                .map(String::from)
                .chain(provided_args.iter().cloned())
                .collect();

            if has_dash_dash {
                return result;
            }

            // replace all but argv0
            result.iter().take(1)
                .map(String::from)
                .chain(provided_args.iter().cloned())
                .collect()

        } else {
            args.take_while(|x| x != &"--")
                .map(replace_argv0)
                .map(String::from)
                .chain(provided_args.iter().cloned())
                .collect()
        }
    }

}

fn display_output(file: &Path) -> Result<()> {
    std::fs::File::open(file)
        .and_then(|mut f| std::io::copy(&mut f, &mut std::io::stdout().lock()))
        .map_err(|e| Error::UnableToReadOutfile(file.display().to_string(), e))?;
    Ok(())
}

struct ProcessRunner {
}

impl Runner for ProcessRunner {
    fn run(&self, cmd: Vec<String>, cd: &Option<PathBuf>) -> Result<RetCode> {

        if let Some((command, args)) = cmd.split_first() {
            let mut exec = Command::new(command);
            exec.args(args);

            // TODO - was .inspect(), but not available in 1.63
            if let Some(ref d) = cd.as_ref() {
                exec.current_dir(d);
            }

            let result = exec.status()
                .map_err(Error::FailedToExec)?;

            match result.code() {
                Some(c) => {
                    Ok(RetCode::try_from(c).expect("isize couldn't contain i32"))
                },
                None => Err(Self::no_result_code(result))
            }

        } else {
            Err(Error::EmptyEntry)
        }
    }

    fn display_output(&self, file: &Path) -> Result<()> {
        display_output(file)
    }

    fn display(&self, s: &str) {
        println!("{}", s)
    }

}

impl ProcessRunner {
    #[cfg(target_family = "unix")]
    fn no_result_code(result: std::process::ExitStatus) -> Error {
        use std::os::unix::process::ExitStatusExt;
        Error::ExitWithSignal(result.signal().unwrap().try_into().unwrap())
    }

    #[cfg(not(target_family = "unix"))]
    fn no_result_code(_result: std::process::ExitStatus) -> Error {
        Error::ExitWithSignal(127)
    }
}

struct PrintRunner {
}

impl Runner for PrintRunner {
    fn run(&self, cmd: Vec<String>, _cd: &Option<PathBuf>) -> Result<RetCode> {
        println!("{}", cmd.join(" "));
        Ok(0)
    }

    fn display_output(&self, file: &Path) -> Result<()> {
        display_output(file)
    }

    fn display(&self, _s: &str) {
        // PrintRunner doesn't show the commentary
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::{RefCell, RefMut}, collections::{HashSet, VecDeque}, rc::Rc};

    use super::*;

    #[derive(Default, Debug, Clone)]
    struct RunData {
        cmd: Vec<String>,
        cd: Option<PathBuf>,
    }

    #[derive(Default, Debug)]
    struct TestData {
        run_data: VecDeque<RunData>,
        outfile: VecDeque<PathBuf>,
        display: VecDeque<String>,
        result: VecDeque<Result<RetCode>>,
    }

    impl TestData {
        fn clear(&mut self) {
            self.run_data.clear();
            self.outfile.clear();
            self.display.clear();
            self.result.clear();
        }
    }

    #[derive(Debug)]
    struct TestRunner {
        data: Rc<RefCell<TestData>>
    }

    impl TestRunner {
        fn new(data: Rc<RefCell<TestData>>) -> TestRunner {
            TestRunner {
                data
            }
        }
    }

    impl Runner for TestRunner {
        fn run(&self, cmd: Vec<String>, cd: &Option<PathBuf>) -> Result<RetCode> {
            let mut data = self.data.borrow_mut();
            println!("run cmd={:#?} cd={:#?} result={:#?}", cmd, cd, data.result.front());
            data.run_data.push_back(RunData{cmd, cd: cd.clone()});
            data.result.pop_front().expect("Result wasn't set")
        }

        fn display_output(&self, file: &Path) -> Result<()> {
            let mut data = self.data.borrow_mut();
            data.outfile.push_back(PathBuf::from(file));
            Ok(())
        }

        fn display(&self, s: &str) {
            let mut data = self.data.borrow_mut();
            data.display.push_back(String::from(s));
        }
    }

    struct TestRun {
        test_data: Rc<RefCell<TestData>>,
        cfg: Config,
    }

    impl TestRun {
        fn new() -> TestRun {
            TestRun {
                test_data: Rc::new(RefCell::new(TestData::default())),
                cfg: Config::default(),
            }
        }

        fn override_argv0<T: Into<String>>(&mut self, a: T) -> &mut Self {
            self.cfg.argv0 = a.into();
            self
        }

        fn select<const N: usize>(&mut self, tags: [&str ;N]) -> &mut Self {
            self.cfg.select = HashSet::from(tags.map(|x| x.to_string()));
            self
        }

        fn reject<const N: usize>(&mut self, tags: [&str ;N]) -> &mut Self {
            self.cfg.reject = HashSet::from(tags.map(|x| x.to_string()));
            self
        }

        // REVIEW - above calls are mutable, below are not, so you need to chain
        // them first

        fn add_return_data(&self, result: Result<RetCode>) -> &Self {
            let mut data: RefMut<'_, _> = self.test_data.borrow_mut();
            data.result.push_back(result);
            self
        }

        fn run<const N: usize>(&self, file_data: &str, provided_args: [&str; N], expected_result: Result<()>) -> &Self {
            let provided_args: Vec<String> = provided_args.into_iter().map(String::from).collect();
            self.run_(file_data, |e,f| e.run(Path::new(".upbuild"), f, &self.cfg, &provided_args), expected_result)
        }

        fn run_with_path<const N: usize>(&self, path: &str, file_data: &str, provided_args: [&str; N], expected_result: Result<()>) -> &Self {
            let provided_args: Vec<String> = provided_args.into_iter().map(String::from).collect();
            self.run_(file_data, |e,f| e.run(Path::new(path), f, &self.cfg, &provided_args), expected_result)
        }

        fn run_without_args(&self, file_data: &str, expected_result: Result<()>) -> &Self {
            self.run(file_data, [], expected_result)
        }

        fn run_<F>(&self, file_data: &str, f: F, expected_result: Result<()>) -> &Self
        where
            F: FnOnce(Exec, &ClassicFile) -> Result<()>
        {
            let file = ClassicFile::parse_lines(file_data.lines().map(String::from)).unwrap();
            let runner = Box::new(TestRunner::new(self.test_data.clone()));

            let e = Exec::new(runner);

            match expected_result {
                Ok(_) => { f(e, &file).expect("Should pass"); },
                Err(err) => {
                    let ret = f(e, &file).expect_err("Should fail");
                    if let Error::ExitWithExitCode(exp_c) = err {
                        match ret {
                            Error::ExitWithExitCode(c) => {
                                assert_eq!(c, exp_c);
                            },
                            _ => panic!("unmatched exit code {:?}", err)
                        }
                    } else if let Error::ExitWithSignal(exp_sig) = err {
                        match ret {
                            Error::ExitWithSignal(sig) => {
                                assert_eq!(sig, exp_sig);
                            },
                            _ => panic!("unmatched exit signal {:?}", err)
                        }
                    } else {
                        panic!("handled unexpected error {:?}", err)
                    }
                },
            }

            {
                let data: RefMut<'_, _> = self.test_data.borrow_mut();
                assert!(data.result.is_empty(), "Didn't exhaust results {:#?}", data.result);
            }
            self
        }

        fn verify_cd_comment(&self, expected: &str) -> &Self {
            let mut data: RefMut<'_, _> = self.test_data.borrow_mut();
            let s = data.display.pop_front().expect("Expected results");
            assert_eq!(s, expected);
            self
        }

        fn verify_cd_dir<S: AsRef<str>>(&self, dir: S) -> &Self {
            let expected = format!("upbuild: Entering directory `{}'", dir.as_ref());
            self.verify_cd_comment(expected.as_str())
        }

        fn verify_return_data<const N: usize>(&self, cmd: [&str; N], cd: Option<PathBuf>) -> &Self {
            let mut data: RefMut<'_, _> = self.test_data.borrow_mut();
            let result = data.run_data.pop_front().expect("Expected results");
            assert_eq!(result.cmd, cmd);
            assert_eq!(result.cd, cd);
            self
        }

        fn verify_outfile(&self, expected: &str) -> &Self {
            let mut data: RefMut<'_, _> = self.test_data.borrow_mut();
            let outfile = data.outfile.pop_front();
            assert_eq!(PathBuf::from(expected), outfile.expect("expected outfile"));
            self
        }

        fn verify_complete(&self) {
            let data: RefMut<'_, _> = self.test_data.borrow_mut();
            assert!(data.run_data.is_empty(), "Didn't exhaust run_data {:#?}", data.run_data);
            assert!(data.outfile.is_empty(), "Didn't exhaust outfile {:#?}", data.outfile);
            assert!(data.display.is_empty(), "Didn't exhaust display {:#?}", data.display);
            assert!(data.result.is_empty());
        }

        fn done(&self) {
            self.verify_complete();
            let mut data: RefMut<'_, _> = self.test_data.borrow_mut();
            data.clear();
        }
    }

    #[test]
    fn test_exec_uv4() {

        let file_data = include_str!("../tests/uv4.upbuild");
        let uv4_run = ["uv4", "-j0", "-b", "project.uvproj", "-o", "log.txt"];
        TestRun::new()
            .add_return_data(Ok(0))
            .run_without_args(file_data, Ok(()))
            .verify_return_data(uv4_run, None)
            .verify_outfile("log.txt")
            .done();

        // 1 should map to 0
        TestRun::new()
            .add_return_data(Ok(1))
            .run_without_args(file_data, Ok(()))
            .verify_return_data(uv4_run, None)
            .verify_outfile("log.txt")
            .done();

        // 2 should fail though
        TestRun::new()
            .add_return_data(Ok(2))
            .run_without_args(file_data, Err(Error::ExitWithExitCode(2)))
            .verify_return_data(uv4_run, None)
            .done();

        // signals should be propagated
        TestRun::new()
            .add_return_data(Err(Error::ExitWithSignal(6)))
            .run_without_args(file_data, Err(Error::ExitWithSignal(6)))
            .verify_return_data(uv4_run, None)
            .done();
    }

    #[test]
    fn test_exec_tags() {
        let file_data = include_str!("../tests/manual.upbuild");
        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run_without_args(file_data, Ok(()))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["make", "cross"], None)
            .done();

        TestRun::new()
            .add_return_data(Ok(1))
            .run_without_args(file_data, Err(Error::ExitWithExitCode(1)))
            .verify_return_data(["make", "tests"], None)
            .done();

        // select hosts tags
        TestRun::new()
            .select(["host"])
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run_without_args(file_data, Ok(()))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["make", "install"], None)
            .done();

        TestRun::new()
            .select(["release"])
            .add_return_data(Ok(0))
            .run_without_args(file_data, Ok(()))
            .verify_return_data(["make", "install"], None)
            .done();

        TestRun::new()
            .select(["target"])
            .add_return_data(Ok(0))
            .run_without_args(file_data, Ok(()))
            .verify_return_data(["make", "cross"], None)
            .done();

        TestRun::new()
            .select(["target", "host"])
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run_without_args(file_data, Ok(()))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["make", "cross"], None)
            .verify_return_data(["make", "install"], None)
            .done();

        TestRun::new()
            .select(["target", "host"])
            .add_return_data(Ok(0))
            .add_return_data(Ok(1))
            .run_without_args(file_data, Err(Error::ExitWithExitCode(1)))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["make", "cross"], None)
            .done();

        TestRun::new()
            .select(["host"])
            .reject(["release"])
            .add_return_data(Ok(0))
            .run_without_args(file_data, Ok(()))
            .verify_return_data(["make", "tests"], None)
            .done();

        TestRun::new()
            .reject(["host"])
            .add_return_data(Ok(0))
            .run_without_args(file_data, Ok(()))
            .verify_return_data(["make", "cross"], None)
            .done();

        TestRun::new()
            .select(["target"])
            .add_return_data(Ok(0))
            .run_without_args(file_data, Ok(()))
            .verify_return_data(["make", "cross"], None)
            .done();
    }

    #[test]
    fn args() {
        let file_data = include_str!("../tests/args.upbuild");
        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run(file_data, [], Ok(()))
            .verify_return_data(["make", "-j8", "BUILD_MODE=host_debug", "test"], None)
            .verify_return_data(["echo", "foo"], None)
            .done();

        if crate::OLD_STYLE_ARGS_HANDLER {

            TestRun::new()
                .add_return_data(Ok(0))
                .add_return_data(Ok(0))
                .run(file_data, ["all"], Ok(()))
                .verify_return_data(["make", "-j8", "BUILD_MODE=host_debug", "all"], None)
                .verify_return_data(["echo", "all"], None)
                .done();

            TestRun::new()
                .add_return_data(Ok(0))
                .add_return_data(Ok(0))
                .run(file_data, ["all", "tests"], Ok(()))
                .verify_return_data(["make", "-j8", "BUILD_MODE=host_debug", "all", "tests"], None)
                .verify_return_data(["echo", "all", "tests"], None)
                .done();

            return;
        }

        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run(file_data, ["all"], Ok(()))
            .verify_return_data(["make", "-j8", "BUILD_MODE=host_debug", "all"], None)
            .verify_return_data(["echo", "foo", "all"], None)
            .done();

        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run(file_data, ["all", "tests"], Ok(()))
            .verify_return_data(["make", "-j8", "BUILD_MODE=host_debug", "all", "tests"], None)
            .verify_return_data(["echo", "foo", "all", "tests"], None)
            .done();
    }

    #[test]
    fn recurse() {
        let file_data = include_str!("../tests/recurse.upbuild");
        let dot_dot_path = PathBuf::from("..").canonicalize().unwrap();
        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run(file_data, [], Ok(()))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["upbuild"], Some(PathBuf::from("..")))
            .verify_cd_dir(dot_dot_path.display().to_string().as_str())
            .done();

        TestRun::new()
            .override_argv0("/path/to/upbuild")
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run(file_data, [], Ok(()))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["/path/to/upbuild"], Some(PathBuf::from("..")))
            .verify_cd_dir(dot_dot_path.display().to_string().as_str())
            .done();

        let file_data = include_str!("../tests/norecurse.upbuild");
        TestRun::new()
            .override_argv0("/path/to/upbuild")
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run(file_data, [], Ok(()))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["/path/to/upbuild"], Some(PathBuf::from("/path/to/build")))
            .verify_cd_dir("/path/to/build")
            .done();
    }

    #[test]
    fn non_local() {
        let file_data = include_str!("../tests/manual.upbuild");

        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run_with_path(".upbuild", file_data, [], Ok(()))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["make", "cross"], None)
            .done();

        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run_with_path("./upbuild", file_data, [], Ok(()))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["make", "cross"], None)
            .done();

        let dot_dot_path = PathBuf::from("..").canonicalize().unwrap().display().to_string();
        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run_with_path("../.upbuild", file_data, [], Ok(()))
            .verify_return_data(["make", "tests"], Some("..".into()))
            .verify_return_data(["make", "cross"], Some("..".into()))
            .verify_cd_dir(&dot_dot_path)
            .done();

        // Should show when we revert back to original dir (if it wasn't already printed)
        let dot_path = PathBuf::from(".").canonicalize().unwrap().display().to_string();
        let file_data = include_str!("../tests/cd.upbuild");
        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run(file_data, [], Ok(()))
            .verify_return_data(["echo", "1"], None)
            .verify_return_data(["echo", "2"], Some("/some/dir".into()))
            .verify_return_data(["echo", "3"], Some("/some/dir".into()))
            .verify_return_data(["echo", "4"], None)
            .verify_return_data(["echo", "5"], Some("/some/dir".into()))
            .verify_return_data(["echo", "6"], Some("/some/other/dir".into()))
            .verify_return_data(["echo", "7"], None)
            .verify_cd_dir("/some/dir")
            .verify_cd_dir(&dot_path)
            .verify_cd_dir("/some/dir")
            .verify_cd_dir("/some/other/dir")
            .verify_cd_dir(&dot_path)
            .done();

        // Should show when we revert back to original dir (if it wasalready printed)
        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run_with_path("../.upbuild", file_data, [], Ok(()))
            .verify_return_data(["echo", "1"], Some("..".into()))
            .verify_return_data(["echo", "2"], Some("/some/dir".into()))
            .verify_return_data(["echo", "3"], Some("/some/dir".into()))
            .verify_return_data(["echo", "4"], Some("..".into()))
            .verify_return_data(["echo", "5"], Some("/some/dir".into()))
            .verify_return_data(["echo", "6"], Some("/some/other/dir".into()))
            .verify_return_data(["echo", "7"], Some("..".into()))
            .verify_cd_dir(&dot_dot_path)
            .verify_cd_dir("/some/dir")
            .verify_cd_dir(&dot_dot_path)
            .verify_cd_dir("/some/dir")
            .verify_cd_dir("/some/other/dir")
            .verify_cd_dir(&dot_dot_path)
            .done();
    }
}
