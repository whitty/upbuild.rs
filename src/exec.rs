use super::{Error, Result};
use super::file::ClassicFile;

use std::collections::HashSet;

use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub type RetCode = isize;

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
        self.run_with_tags(file, &tags)
    }

    /// Run the given classic file and selected tags
    pub fn run_with_tags(&self, file: &ClassicFile, tags: &HashSet<String>) -> Result<()> {
        for cmd in &file.commands {
            if ! cmd.enabled(&tags) {
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

pub struct ProcessRunner {
}

impl Runner for ProcessRunner {
    fn run(&self, cmd: Vec<String>, cd: Option<PathBuf>) -> Result<RetCode> {

        if let Some((command, args)) = cmd.split_first() {
            let mut exec = Command::new(command);
            exec.args(args);

            cd.inspect(|ref d| { exec.current_dir(d); });

            let result = exec.status()
                .map_err(|e| Error::FailedToExec(e))?;

            match result.code() {
                Some(c) => {
                    Ok(RetCode::try_from(c).expect("isize couldn't contain i32"))
                },
                None => return Err(Error::ExitWithSignal((result.signal().unwrap() as i32).try_into().unwrap()))
            }

        } else {
            return Err(Error::EmptyEntry);
        }
    }

    fn display_output(&self, _file: &Path) -> Result<()>
    {
        todo!("@outfile not yet implemented {}", _file.display());
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::{RefCell, RefMut}, rc::Rc};

    use super::*;

    #[derive(Debug)]
    struct TestRunner {
        data: Rc<RefCell<TestData>>
    }

    #[derive(Default, Debug)]
    struct TestData {
        // TODO - just a single one!
        cmd: Option<Vec<String>>,
        cd: Option<PathBuf>,
        outfile: Option<PathBuf>,
        result: Option<Result<RetCode>>,
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
            data.cmd.replace(cmd);
            data.cd = cd;
            data.result.take().expect("Result wasn't set")
        }

        fn display_output(&self, file: &Path) -> Result<()>
        {
            let mut data = self.data.borrow_mut();
            data.outfile.replace(PathBuf::from(file));
            Ok(())
        }
    }

    fn set_return_data(test_data: &Rc<RefCell<TestData>>, result: Result<RetCode>) {
        let mut data: RefMut<'_, _> = test_data.borrow_mut();
        data.result.replace(result);
    }

    fn get_call_data(test_data: &Rc<RefCell<TestData>>) -> (Option<Vec<String>>, Option<PathBuf>) {
        let mut data: RefMut<'_, _> = test_data.borrow_mut();
        (data.cmd.take(), data.cd.take())
    }

    fn get_outfile(test_data: &Rc<RefCell<TestData>>) -> Option<PathBuf> {
        let mut data: RefMut<'_, _> = test_data.borrow_mut();
        data.outfile.take()
    }

    fn simple_test<const N: usize>(
        file_data: &str, result: Result<RetCode>, expected_result: Result<()>,
        expected_cmd: [&str; N], expected_cd: Option<PathBuf>)
    {
        simple_test_(file_data, result, expected_result, expected_cmd, expected_cd, None)
    }

    fn simple_test_outfile<const N: usize>(
        file_data: &str, result: Result<RetCode>, expected_result: Result<()>,
        expected_cmd: [&str; N], expected_cd: Option<PathBuf>, expected_outfile: &str)
    {
        let outfile = PathBuf::from(expected_outfile);
        simple_test_(file_data, result, expected_result, expected_cmd, expected_cd, Some(&outfile))
    }

    fn simple_test_<const N: usize>(
        file_data: &str, result: Result<RetCode>, expected_result: Result<()>,
        expected_cmd: [&str; N], expected_cd: Option<PathBuf>, expected_outfile: Option<&Path>)
    {
        let test_data: Rc<RefCell<_>> = Rc::new(RefCell::new(TestData::default()));
        set_return_data(&test_data, result);

        let file = ClassicFile::parse_lines(file_data.split_terminator("\n")).unwrap();
        let runner = Box::new(TestRunner::new(test_data.clone()));

        let e = Exec::new(runner);
        match expected_result {
            Ok(_) => { e.run(&file).expect("Should pass"); },
            Err(err) => {
                let ret = e.run(&file).expect_err("Should fail");
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

        let (cmd, cd) = get_call_data(&test_data);
        println!("cmd={:#?} cd={:#?}", cmd, cd);
        assert_eq!(cmd.unwrap(), expected_cmd);
        assert_eq!(cd, expected_cd);

        let outfile = get_outfile(&test_data);
        match outfile {
            Some(f) => {
                println!("outfile=Some({:?})", f);
                assert_eq!(expected_outfile.expect("expected None"), f)
            },
            None => assert_eq!(expected_outfile, None)
        }
    }

    #[test]
    fn test_exec_uv4() {
        simple_test_outfile(include_str!("../tests/uv4.upbuild"), Ok(0), Ok(()),
                            ["uv4", "-j0", "-b", "project.uvproj", "-o", "log.txt"],
                            None, "log.txt");

        // 1 should map to 0
        simple_test_outfile(include_str!("../tests/uv4.upbuild"), Ok(1), Ok(()),
                            ["uv4", "-j0", "-b", "project.uvproj", "-o", "log.txt"],
                            None, "log.txt");

        // 2 should fail though
        simple_test(include_str!("../tests/uv4.upbuild"), Ok(2), Err(Error::ExitWithExitCode(2)),
                            ["uv4", "-j0", "-b", "project.uvproj", "-o", "log.txt"],
                            None);

        // signals should be propagated
        simple_test(include_str!("../tests/uv4.upbuild"), Err(Error::ExitWithSignal(6)), Err(Error::ExitWithSignal(6)),
                            ["uv4", "-j0", "-b", "project.uvproj", "-o", "log.txt"],
                            None);
    }
}
