use std::{fs, os::linux::fs::MetadataExt, path::PathBuf};
use super::{Error, Result};

fn readable(p: &PathBuf) -> bool {
    ! fs::File::open(p).is_err()
}

fn inode(p: &PathBuf) -> u64 {
    fs::metadata(p).unwrap().st_ino()
}

// Ensure we don't recurse forever
const MAX_DEPTH: usize = 128;

pub fn find(start: &str) -> Result<PathBuf> {
    let mut curr = PathBuf::from(start);
    if ! curr.is_dir() {
        return Err(Error::InvalidDir(curr.display().to_string()));
    }

    for _ in 0..MAX_DEPTH {
        curr.push(".upbuild");
        if curr.is_file() && readable(&curr) {
            return Ok(curr)
        }
        curr.pop();

        let i = inode(&curr);
        curr.push("..");

        if ! curr.is_dir() {
            break;
        }
        if i == inode(&curr) {
            // reached the root level
            break;
        }
    }

    Err(Error::NotFound(start.to_string()))
}
