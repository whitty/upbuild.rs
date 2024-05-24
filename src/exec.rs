use super::{Error, Result, Config};
use super::file::ClassicFile;

use std::os::unix::process::ExitStatusExt;
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
    fn run(&self, cmd: Vec<String>, cd: Option<PathBuf>) -> Result<RetCode>;

    fn display_output(&self, file: &Path) -> Result<()>;
}

impl Exec {

    /// Create a new executor with the given Runner as environment
    pub fn new(runner: Box<dyn Runner>) -> Self {
        Self { runner }
    }

    /// Run the given classic file, args, and config
    pub fn run_with_config(&self, file: &ClassicFile, cfg: &Config, provided_args: &[String]) -> Result<()> {
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

            let code = self.runner.run(args, cmd.directory())?;
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

    fn with_args(args: std::slice::Iter<'_, String>, provided_args: &[String], argv0: Option<&String>) -> Vec<String> {
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

        if provided_args.is_empty() {

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

        args.take_while(|x| x != &"--")
            .map(replace_argv0)
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

struct ProcessRunner {
}

impl Runner for ProcessRunner {
    fn run(&self, cmd: Vec<String>, cd: Option<PathBuf>) -> Result<RetCode> {

        if let Some((command, args)) = cmd.split_first() {
            let mut exec = Command::new(command);
            exec.args(args);

            cd.inspect(|ref d| { exec.current_dir(d); });

            let result = exec.status()
                .map_err(Error::FailedToExec)?;

            match result.code() {
                Some(c) => {
                    Ok(RetCode::try_from(c).expect("isize couldn't contain i32"))
                },
                None => Err(Error::ExitWithSignal(result.signal().unwrap().try_into().unwrap()))
            }

        } else {
            Err(Error::EmptyEntry)
        }
    }

    fn display_output(&self, file: &Path) -> Result<()> {
        display_output(file)
    }
}

struct PrintRunner {
}

impl Runner for PrintRunner {
    fn run(&self, cmd: Vec<String>, _cd: Option<PathBuf>) -> Result<RetCode> {
        println!("{}", cmd.join(" "));
        Ok(0)
    }

    fn display_output(&self, file: &Path) -> Result<()> {
        display_output(file)
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
        result: VecDeque<Result<RetCode>>,
    }

    impl TestData {
        fn clear(&mut self) {
            self.run_data.clear();
            self.outfile.clear();
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
        fn run(&self, cmd: Vec<String>, cd: Option<PathBuf>) -> Result<RetCode> {
            let mut data = self.data.borrow_mut();
            println!("run cmd={:#?} cd={:#?} result={:#?}", cmd, cd, data.result);
            data.run_data.push_back(RunData{cmd, cd});
            data.result.pop_front().expect("Result wasn't set")
        }

        fn display_output(&self, file: &Path) -> Result<()> {
            let mut data = self.data.borrow_mut();
            data.outfile.push_back(PathBuf::from(file));
            Ok(())
        }
    }

    struct TestRun {
        test_data: Rc<RefCell<TestData>>,
        argv0: Option<String>,
    }

    impl TestRun {
        fn new() -> TestRun {
            TestRun {
                test_data: Rc::new(RefCell::new(TestData::default())),
                argv0: None
            }
        }

        fn override_argv0<T: Into<String>>(&mut self, a: T) -> &Self {
            self.argv0.replace(a.into());
            self
        }

        fn add_return_data(&self, result: Result<RetCode>) -> &Self {
            let mut data: RefMut<'_, _> = self.test_data.borrow_mut();
            data.result.push_back(result);
            self
        }

        fn run_with_tags<const N: usize, const O: usize>(&self, file_data: &str, select_tags: [&str ;N], reject_tags: [&str ;O], expected_result: Result<()>) -> &Self {
            self.run_with_tags_and_args(file_data, select_tags, reject_tags, [], expected_result)
        }

        fn run_with_args<const N: usize>(&self, file_data: &str, provided_args: [&str; N], expected_result: Result<()>) -> &Self {
            self.run_with_tags_and_args(file_data, [], [], provided_args, expected_result)
        }

        fn run_with_tags_and_args<const N: usize, const O: usize, const Q: usize>(&self, file_data: &str, select_tags: [&str ;N], reject_tags: [&str ;O], provided_args: [&str; Q], expected_result: Result<()>) -> &Self {

            let cfg = Config {
                argv0: self.argv0.clone().unwrap_or(String::from("upbuild")),
                select: HashSet::from(select_tags.map(|x| x.to_string())),
                reject: HashSet::from(reject_tags.map(|x| x.to_string())),
                ..Default::default()
            };

            let provided_args: Vec<String> = provided_args.into_iter().map(String::from).collect();
            self.run_(file_data, |e,f| e.run_with_config(f, &cfg, &provided_args), expected_result)
        }

        fn run_with_select_tags<const N: usize>(&self, file_data: &str, select_tags: [&str ;N], expected_result: Result<()>) -> &Self {
            let cfg = Config {
                argv0: self.argv0.clone().unwrap_or(String::from("upbuild")),
                select: HashSet::from(select_tags.map(|x| x.to_string())),
                ..Default::default()
            };
            self.run_(file_data, |e,f| e.run_with_config(f, &cfg, &[]), expected_result)
        }

        fn run(&self, file_data: &str, expected_result: Result<()>) -> &Self {
            let cfg = Config {
                argv0: self.argv0.clone().unwrap_or(String::from("upbuild")),
                ..Default::default()
            };
            self.run_(file_data, |e,f| e.run_with_config(f, &cfg, &[]), expected_result)
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
            .run(file_data, Ok(()))
            .verify_return_data(uv4_run, None)
            .verify_outfile("log.txt")
            .done();

        // 1 should map to 0
        TestRun::new()
            .add_return_data(Ok(1))
            .run(file_data, Ok(()))
            .verify_return_data(uv4_run, None)
            .verify_outfile("log.txt")
            .done();

        // 2 should fail though
        TestRun::new()
            .add_return_data(Ok(2))
            .run(file_data, Err(Error::ExitWithExitCode(2)))
            .verify_return_data(uv4_run, None)
            .done();

        // signals should be propagated
        TestRun::new()
            .add_return_data(Err(Error::ExitWithSignal(6)))
            .run(file_data, Err(Error::ExitWithSignal(6)))
            .verify_return_data(uv4_run, None)
            .done();
    }

    #[test]
    fn test_exec_tags() {
        let file_data = include_str!("../tests/manual.upbuild");
        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run(file_data, Ok(()))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["make", "cross"], None)
            .done();

        TestRun::new()
            .add_return_data(Ok(1))
            .run(file_data, Err(Error::ExitWithExitCode(1)))
            .verify_return_data(["make", "tests"], None)
            .done();

        // select hosts tags
        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run_with_select_tags(file_data, ["host"], Ok(()))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["make", "install"], None)
            .done();

        TestRun::new()
            .add_return_data(Ok(0))
            .run_with_select_tags(file_data, ["release"], Ok(()))
            .verify_return_data(["make", "install"], None)
            .done();

        TestRun::new()
            .add_return_data(Ok(0))
            .run_with_select_tags(file_data, ["target"], Ok(()))
            .verify_return_data(["make", "cross"], None)
            .done();

        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run_with_select_tags(file_data, ["target", "host"], Ok(()))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["make", "cross"], None)
            .verify_return_data(["make", "install"], None)
            .done();

        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(1))
            .run_with_select_tags(file_data, ["target", "host"], Err(Error::ExitWithExitCode(1)))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["make", "cross"], None)
            .done();

        TestRun::new()
            .add_return_data(Ok(0))
            .run_with_tags(file_data, ["host"], ["release"], Ok(()))
            .verify_return_data(["make", "tests"], None)
            .done();

        TestRun::new()
            .add_return_data(Ok(0))
            .run_with_tags(file_data, [], ["host"], Ok(()))
            .verify_return_data(["make", "cross"], None)
            .done();

        TestRun::new()
            .add_return_data(Ok(0))
            .run_with_tags(file_data, ["target"], [], Ok(()))
            .verify_return_data(["make", "cross"], None)
            .done();
    }

    #[test]
    fn args() {
        let file_data = include_str!("../tests/args.upbuild");
        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run_with_args(file_data, [], Ok(()))
            .verify_return_data(["make", "-j8", "BUILD_MODE=host_debug", "test"], None)
            .verify_return_data(["echo"], None)
            .done();

        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run_with_args(file_data, ["all"], Ok(()))
            .verify_return_data(["make", "-j8", "BUILD_MODE=host_debug", "all"], None)
            .verify_return_data(["echo", "all"], None)
            .done();

        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run_with_args(file_data, ["all", "tests"], Ok(()))
            .verify_return_data(["make", "-j8", "BUILD_MODE=host_debug", "all", "tests"], None)
            .verify_return_data(["echo", "all", "tests"], None)
            .done();
    }

    #[test]
    fn recurse() {
        let file_data = include_str!("../tests/recurse.upbuild");
        TestRun::new()
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run_with_args(file_data, [], Ok(()))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["upbuild"], Some(PathBuf::from("..")))
            .done();

        TestRun::new()
            .override_argv0("/path/to/upbuild")
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run_with_args(file_data, [], Ok(()))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["/path/to/upbuild"], Some(PathBuf::from("..")))
            .done();

        let file_data = include_str!("../tests/norecurse.upbuild");
        TestRun::new()
            .override_argv0("/path/to/upbuild")
            .add_return_data(Ok(0))
            .add_return_data(Ok(0))
            .run_with_args(file_data, [], Ok(()))
            .verify_return_data(["make", "tests"], None)
            .verify_return_data(["/path/to/upbuild"], Some(PathBuf::from("/path/to/build")))
            .done();
    }
}
