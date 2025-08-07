// SPDX-License-Identifier: GPL-3.0-or-later
// (C) Copyright 2024-2025 Greg Whiteley

use super::{Error, Result, Config};
use super::file::ClassicFile;
use super::file::Header;
use super::error::from_dotenvy;

use std::path::{Path, PathBuf};
use std::process::Command;

pub type RetCode = isize;

/// Create a normal runner for [`Exec`] that actually runs the commands
pub fn process_runner() -> Box<dyn Runner> {
   Box::<ProcessRunner>::default()
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

    /// Create given directory if it doesn't exist
    fn check_mkdir(&self, d: &Path) -> Result<()>;

    /// Display output from a file defined by @outfile
    fn display_output(&self, file: &Path) -> Result<()>;

    /// Output additional data
    fn display(&self, s: &str);

    /// Note when we are changing directories
    fn on_enter_dir(&self, dir: &Path) {
        self.display(format!("upbuild: Entering directory `{}'", dir.display()).as_str());
    }

    /// Search for given dotenv filename, optionally ignoring missing
    fn load_global_dotenv_(&self, name: &str, allow_missing: bool) -> Result<()>;

    /// Search for given dotenv filename
    fn load_global_dotenv(&self, name: &str) -> Result<()> {
        self.load_global_dotenv_(name, false)
    }

    /// Search for given dotenv filename
    fn load_default_dotenv(&self) -> Result<()> {
        self.load_global_dotenv_(".upbuild.env", true)
    }
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
            self.runner.on_enter_dir(dir);
        }
    }

    fn show_entering_always(&self, working_dir: &Option<PathBuf>) {
        if working_dir.is_none() {
            let dot = Some(PathBuf::from("."));
            return self.show_entering(&dot);
        }
        self.show_entering(working_dir)
    }

    fn run_dir(main_working_dir: &Option<PathBuf>, cmd_dir: Option<PathBuf>) -> Option<PathBuf> {
        match cmd_dir {
            Some(d) => {
                match main_working_dir {
                    Some(m) => Some(m.join(d)), // join squashes LHS if RHS is absolute
                    None => Some(d),
                }
            },
            None => main_working_dir.clone() // TODO clones
        }
    }

    fn apply_header(&self, header: &Header, cfg: &Config) -> Result<()> {
        if !cfg.skip_env && header.dotenv().is_empty() {
            // By default we look for .upbuild.env, but squash failure to read it
            self.runner.load_default_dotenv()?;
        } else {
            for d in header.dotenv() {
                self.runner.load_global_dotenv(d)?;
            }
        }
        Ok(())
    }

    /// Run the given classic file, args, and config
    pub fn run(&self, path: &Path, file: &ClassicFile, cfg: &Config, provided_args: &[String]) -> Result<()> {

        self.apply_header(&file.header, cfg)?;

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
                                       }
            );

            let mk_dir = cmd.mk_dir();
            if mk_dir.is_some() {
                if let Some(d) = Self::run_dir(&main_working_dir, mk_dir) {
                    if let Err(x) = self.runner.check_mkdir(&d) {
                        eprintln!("Failed to create directory {}: {}", d.display(), x)
                    }
                }
            }

            let cmd_dir = cmd.directory();
            let run_dir = Self::run_dir(&main_working_dir, cmd_dir);

            if run_dir != last_dir {
                self.show_entering_always(&run_dir); // after initial cd always show any change
                last_dir.clone_from(&run_dir); // TODO clones
            }

            let code = self.runner.run(args, &run_dir)?;
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

    fn with_args(args: &[String], provided_args: &[String], argv0: Option<&String>) -> Vec<String> {

        let skip = if argv0.is_some() { 1 } else { 0 };

        if provided_args.is_empty() {

            let mut first_separator = true;
            return argv0.into_iter()
                .chain(args.iter().skip(skip))
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

        argv0.into_iter()
            .chain(args.iter().skip(skip))
            .take_while(|x| x != &"--")
            .map(String::from)
            .chain(provided_args.iter().cloned())
            .collect()
    }

}

fn display_output(file: &Path) -> Result<()> {
    std::fs::File::open(file)
        .and_then(|mut f| std::io::copy(&mut f, &mut std::io::stdout().lock()))
        .map_err(|e| Error::UnableToReadOutfile(file.display().to_string(), e))?;
    Ok(())
}

#[derive(Default)]
struct ProcessRunner {
}

impl Runner for ProcessRunner {
    fn run(&self, cmd: Vec<String>, cd: &Option<PathBuf>) -> Result<RetCode> {

        if let Some((command, args)) = cmd.split_first() {
            let mut exec = Command::new(command);

            // On windows std::process::Command evaluates the
            // executable _before_ the `current_dir()` is applied
            if cfg!(windows) {
                let bin = Path::new(command);
                if bin.is_relative() && cd.is_some() {
                    let base = cd.as_ref().unwrap();
                    let cmd_path = base.as_path().join(command);

                    // bin.is_relative() finds non-path prefixed
                    // commands ie "hello" is non-path prefixed.  So
                    // drop case where file-name is the entire file.
                    // EXCEPT - that means dropping the case where we
                    // @cd to a directory, then run locally.
                    //
                    // So replicate DOS behaviour manually and resolve
                    // to the exe if it exists in the @cd dir.

                    if Some(bin.as_os_str()) != bin.file_name() ||
                        cmd_path.exists() {
                        exec = Command::new(cmd_path);
                    }
                }
            }
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

    fn check_mkdir(&self, d: &Path) -> Result<()> {
        if d.is_dir() {
            return Ok(());
        }
        std::fs::create_dir_all(d).map_err(Error::IoFailed)
    }

    fn load_global_dotenv_(&self, name: &str, allow_missing: bool) -> Result<()> {
        if allow_missing {
            dotenvy::from_filename_override(name)
                .map(|_| ()) // squash the value
                .or_else(|e|
                         match e {
                             dotenvy::Error::Io(_) => Ok(()), // Read failures are OK
                             _ => Err(from_dotenvy(name.to_string(), e)),
                         })?;
        } else {
            dotenvy::from_filename_override(name)
                .map_err(|e| from_dotenvy(name.to_string(), e))?;
        }
        Ok(())
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

#[cfg(target_family = "unix")]
const COMMENT: &str = "#";
#[cfg(not(target_family = "unix"))]
const COMMENT: &str = "rem";

impl Runner for PrintRunner {
    fn run(&self, cmd: Vec<String>, _cd: &Option<PathBuf>) -> Result<RetCode> {
        println!("{}", cmd.join(" "));
        Ok(0)
    }

    fn check_mkdir(&self, d: &Path) -> Result<()> {
        println!("{} Checking existence of directory {}", COMMENT, d.display());
        Ok(())
    }

    fn display_output(&self, file: &Path) -> Result<()> {
        display_output(file)
    }

    fn display(&self, _s: &str) {
        // PrintRunner doesn't show the commentary
    }

    fn on_enter_dir(&self, s: &Path) {
        println!("{} cd '{}'", COMMENT, s.display())
    }

    fn load_global_dotenv_(&self, name: &str, allow_missing: bool) -> Result<()> {
        if allow_missing {
            // REVIEW - for now be quiet on default load
            // println!("{} would load default env from '{}' if present", COMMENT, name)
        } else {
            println!("{} would load env from '{}'", COMMENT, name)
        }
        Ok(())
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
        mkdir: VecDeque<PathBuf>,
    }

    impl TestData {
        fn clear(&mut self) {
            self.run_data.clear();
            self.outfile.clear();
            self.display.clear();
            self.result.clear();
            self.mkdir.clear();
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

        fn check_mkdir(&self, d: &Path) -> Result<()> {
            let mut data = self.data.borrow_mut();
            data.mkdir.push_back(PathBuf::from(d));
            Ok(())
        }

        fn load_global_dotenv_(&self, _name: &str, _allow_missing: bool) -> Result<()> {
            Ok(())
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
            let file = ClassicFile::parse_lines(file_data.lines()).unwrap();
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

        fn verify_mkdir(&self, expected: &str) -> &Self {
            let mut data: RefMut<'_, _> = self.test_data.borrow_mut();
            let outfile = data.mkdir.pop_front();
            assert_eq!(PathBuf::from(expected), outfile.expect("expected mkdir"));
            self
        }

        fn verify_complete(&self) {
            let data: RefMut<'_, _> = self.test_data.borrow_mut();
            assert!(data.run_data.is_empty(), "Didn't exhaust run_data {:#?}", data.run_data);
            assert!(data.outfile.is_empty(), "Didn't exhaust outfile {:#?}", data.outfile);
            assert!(data.display.is_empty(), "Didn't exhaust display {:#?}", data.display);
            assert!(data.result.is_empty());
            assert!(data.mkdir.is_empty(), "Didn't exhaust mkdir {:#?}", data.mkdir);
        }

        fn done(&self) {
            self.verify_complete();
            let mut data: RefMut<'_, _> = self.test_data.borrow_mut();
            data.clear();
        }
    }

    fn args_vec<const N: usize>(provided_args: [&str; N]) -> Vec<String> {
        provided_args.into_iter().map(String::from).collect()
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
    }

    #[test]
    fn cmake() {
        let file_data = include_str!("../tests/cmake.upbuild");

        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run(file_data, [], Ok(()))
            .verify_return_data(["cmake", ".."], Some("build".into()))
            .verify_return_data(["cmake", "--build", "."], Some("build".into()))
            .verify_cd_dir("build")
            .verify_mkdir("build")
            .done();
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn cd() {
        // Should show when we revert back to original dir (if it wasn't already printed)
        let dot_dot_path = PathBuf::from("..").canonicalize().unwrap().display().to_string();
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
            .add_return_data(Ok(0))
            .run(file_data, [], Ok(()))
            .verify_return_data(["echo", "1"], None)
            .verify_return_data(["echo", "2"], Some("/some/dir".into()))
            .verify_return_data(["echo", "3"], Some("/some/dir".into()))
            .verify_return_data(["echo", "4"], None)
            .verify_return_data(["echo", "5"], Some("/some/dir".into()))
            .verify_return_data(["echo", "6"], Some("/some/other/dir".into()))
            .verify_return_data(["echo", "7"], None)
            .verify_return_data(["echo", "8"], some_path("some/subdir"))
            .verify_cd_dir("/some/dir")
            .verify_cd_dir(&dot_path)
            .verify_cd_dir("/some/dir")
            .verify_cd_dir("/some/other/dir")
            .verify_cd_dir(&dot_path)
            .verify_cd_dir("some/subdir")
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
            .add_return_data(Ok(0))
            .run_with_path("../.upbuild", file_data, [], Ok(()))
            .verify_return_data(["echo", "1"], Some("..".into()))
            .verify_return_data(["echo", "2"], Some("/some/dir".into()))
            .verify_return_data(["echo", "3"], Some("/some/dir".into()))
            .verify_return_data(["echo", "4"], Some("..".into()))
            .verify_return_data(["echo", "5"], Some("/some/dir".into()))
            .verify_return_data(["echo", "6"], Some("/some/other/dir".into()))
            .verify_return_data(["echo", "7"], Some("..".into()))
            .verify_return_data(["echo", "8"], some_path("../some/subdir"))
            .verify_cd_dir(&dot_dot_path)
            .verify_cd_dir("/some/dir")
            .verify_cd_dir(&dot_dot_path)
            .verify_cd_dir("/some/dir")
            .verify_cd_dir("/some/other/dir")
            .verify_cd_dir(&dot_dot_path)
            .verify_cd_dir("../some/subdir")
            .done();
    }

    #[test]
    #[cfg(not(target_family = "unix"))]
    fn cd() {
        // Should show when we revert back to original dir (if it wasn't already printed)
        let dot_dot_path = PathBuf::from("..").canonicalize().unwrap().display().to_string();
        let dot_path = PathBuf::from(".").canonicalize().unwrap().display().to_string();
        let file_data = include_str!("../tests/cd.win.upbuild");
        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run(file_data, [], Ok(()))
            .verify_return_data(["echo", "1"], None)
            .verify_return_data(["echo", "2"], Some("\\some\\dir".into()))
            .verify_return_data(["echo", "3"], Some("\\some\\dir".into()))
            .verify_return_data(["echo", "4"], None)
            .verify_return_data(["echo", "5"], Some("\\some\\dir".into()))
            .verify_return_data(["echo", "6"], Some("\\some\\other\\dir".into()))
            .verify_return_data(["echo", "7"], None)
            .verify_return_data(["echo", "8"], some_path("some\\subdir"))
            .verify_cd_dir("\\some\\dir")
            .verify_cd_dir(&dot_path)
            .verify_cd_dir("\\some\\dir")
            .verify_cd_dir("\\some\\other\\dir")
            .verify_cd_dir(&dot_path)
            .verify_cd_dir("some\\subdir")
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
            .add_return_data(Ok(0))
            .run_with_path("..\\.upbuild", file_data, [], Ok(()))
            .verify_return_data(["echo", "1"], Some("..".into()))
            .verify_return_data(["echo", "2"], Some("\\some\\dir".into()))
            .verify_return_data(["echo", "3"], Some("\\some\\dir".into()))
            .verify_return_data(["echo", "4"], Some("..".into()))
            .verify_return_data(["echo", "5"], Some("\\some\\dir".into()))
            .verify_return_data(["echo", "6"], Some("\\some\\other\\dir".into()))
            .verify_return_data(["echo", "7"], Some("..".into()))
            .verify_return_data(["echo", "8"], some_path("..\\some\\subdir"))
            .verify_cd_dir(&dot_dot_path)
            .verify_cd_dir("\\some\\dir")
            .verify_cd_dir(&dot_dot_path)
            .verify_cd_dir("\\some\\dir")
            .verify_cd_dir("\\some\\other\\dir")
            .verify_cd_dir(&dot_dot_path)
            .verify_cd_dir("..\\some\\subdir")
            .done();
    }

    /// result_is_fail if result is error, or code is non-zero
    fn result_is_fail(res: &Result<isize>) -> bool {
        return res.is_err() || *res.as_ref().unwrap() != 0;
    }

    /// On windows std::process::Command evaluates the
    /// executable _before_ the `current_dir()` is applied
    #[test]
    fn process_runner_win32_dir_test() {
        let p = ProcessRunner::default();
        let (comm, path) = if cfg!(windows) { (".\\run.bat", "tests/win/") } else { ("./run.sh", "tests/sh/") };
        let res = p.run(args_vec([comm]), &some_path(path));
        println!("res={:?}", res);
        assert_eq!(res.expect("expected OK"), 0);

        // Try alternate formats to see how the runner works
        if cfg!(windows) {
            let (comm, path) = ("./run.bat", "tests/win/");
            let res = p.run(args_vec([comm]), &some_path(path));
            println!("res={:?}", res);
            assert_eq!(res.expect("expected OK"), 0);

            let (comm, path) = ("./run.bat", "tests\\win\\");
            let res = p.run(args_vec([comm]), &some_path(path));
            println!("res={:?}", res);
            assert_eq!(res.expect("expected OK"), 0);

            // in DOS you don't need ./
            let (comm, path) = ("run.bat", "tests\\win\\");
            let res = p.run(args_vec([comm]), &some_path(path));
            println!("res={:?}", res);
            assert_eq!(res.expect("expected OK"), 0);

            // Ensure it fails if not in
            let (comm, path) = ("run.bat", "tests\\");
            let res = p.run(args_vec([comm]), &some_path(path));
            println!("res={:?}", res);
            assert!(result_is_fail(&res), "Expected fail got {:?}", res);
        }
    }

    #[test]
    fn process_runner_arg_test() {
        let p = ProcessRunner::default();
        let (comm, path) = if cfg!(windows) { (".\\run.bat", "tests/win/") } else { ("./run.sh", "tests/sh/") };
        let res = p.run(args_vec([comm, "1"]), &some_path(path));
        println!("res={:?}", res);
        assert_eq!(res.expect("expected OK(1)"), 1);

        let res = p.run(args_vec([comm, "100"]), &some_path(path));
        println!("res={:?}", res);
        assert_eq!(res.expect("expected OK(100)"), 100);
    }

    fn some_path(s: &str) -> Option<PathBuf> {
        Some(PathBuf::from(s))
    }

    #[test]
    fn run_dir() {
        let main_working_dir = None;
        assert_eq!(Exec::run_dir(&main_working_dir, None), None);
        assert_eq!(Exec::run_dir(&main_working_dir, Some("..".into())), some_path(".."));
        assert_eq!(Exec::run_dir(&main_working_dir, Some("/a".into())), some_path("/a"));

        let main_working_dir = Some(PathBuf::from(".."));
        assert_eq!(Exec::run_dir(&main_working_dir, None), some_path(".."));
        assert_eq!(Exec::run_dir(&main_working_dir, Some("..".into())), some_path("../.."));
        assert_eq!(Exec::run_dir(&main_working_dir, Some("/a".into())), some_path("/a"));

        let main_working_dir = Some(PathBuf::from("/b"));
        assert_eq!(Exec::run_dir(&main_working_dir, None), some_path("/b"));
        assert_eq!(Exec::run_dir(&main_working_dir, Some("..".into())), some_path("/b/.."));
        assert_eq!(Exec::run_dir(&main_working_dir, Some("/a".into())), some_path("/a"));

        let main_working_dir = Some(PathBuf::from("b"));
        assert_eq!(Exec::run_dir(&main_working_dir, None), some_path("b"));
        assert_eq!(Exec::run_dir(&main_working_dir, Some("..".into())), some_path("b/.."));
        assert_eq!(Exec::run_dir(&main_working_dir, Some("/a".into())), some_path("/a"));
    }
}
