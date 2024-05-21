use super::{Error, Result};
use super::file::ClassicFile;

use std::collections::HashSet;

use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub type RetCode = isize;

pub fn process_runner() -> Box<dyn Runner> {
   Box::new(ProcessRunner {})
}

pub fn print_runner() -> Box<dyn Runner> {
   Box::new(PrintRunner {})
}

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

    /// Run the given classic file - no tags
    pub fn run(&self, file: &ClassicFile) -> Result<()> {
        let tags = HashSet::<String>::new();
        self.run_with_tags(file, &tags, &tags)
    }

    /// Run the given classic file and selected tags
    pub fn run_with_tags(&self, file: &ClassicFile, select_tags: &HashSet<String>, reject_tags: &HashSet<String>) -> Result<()> {
        for cmd in &file.commands {
            if ! cmd.enabled_with_reject(select_tags, reject_tags) {
                continue;
            }
            let args = cmd.clone_args();

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
}

fn display_output(_file: &Path) -> Result<()> {
    todo!("@outfile not yet implemented {}", _file.display());
}

pub struct ProcessRunner {
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

pub struct PrintRunner {
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
    use std::{cell::{RefCell, RefMut}, collections::VecDeque, rc::Rc};

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

        fn display_output(&self, file: &Path) -> Result<()>
        {
            let mut data = self.data.borrow_mut();
            data.outfile.push_back(PathBuf::from(file));
            Ok(())
        }
    }

    struct TestRun {
        test_data: Rc<RefCell<TestData>>,
    }

    impl TestRun {
        fn new() -> TestRun {
            TestRun {
                test_data: Rc::new(RefCell::new(TestData::default())),
            }
        }

        fn add_return_data(&self, result: Result<RetCode>) -> &Self {
            let mut data: RefMut<'_, _> = self.test_data.borrow_mut();
            data.result.push_back(result);
            self
        }

        pub fn run_with_tags<const N: usize, const O: usize>(&self, file_data: &str, select_tags: [&str ;N], reject_tags: [&str ;O], expected_result: Result<()>) -> &Self {
            let select_tags = HashSet::from(select_tags.map(|x| x.to_string()));
            let reject_tags = HashSet::from(reject_tags.map(|x| x.to_string()));
            self.run_(file_data, |e,f| e.run_with_tags(f, &select_tags, &reject_tags), expected_result)
        }

        pub fn run_with_select_tags<const N: usize>(&self, file_data: &str, select_tags: [&str ;N], expected_result: Result<()>) -> &Self {
            let tags = HashSet::from(select_tags.map(|x| x.to_string()));
            self.run_(file_data, |e,f| e.run_with_tags(f, &tags, &HashSet::new()), expected_result)
        }

        pub fn run(&self, file_data: &str, expected_result: Result<()>) -> &Self {
            self.run_(file_data, |e,f| e.run(f), expected_result)
        }

        fn run_<F>(&self, file_data: &str, f: F, expected_result: Result<()>) -> &Self
        where
            F: FnOnce(Exec, &ClassicFile) -> Result<()>
        {
            let file = ClassicFile::parse_lines(file_data.split_terminator('\n')).unwrap();
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

}
