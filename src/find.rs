// SPDX-License-Identifier: GPL-3.0-or-later
// (C) Copyright 2024 Greg Whiteley

use std::{fs, path::PathBuf};
use super::{Error, Result};

fn readable(p: &PathBuf) -> bool {
    fs::File::open(p).is_ok()
}

#[cfg(target_family = "unix")]
fn inode(p: &PathBuf) -> u64 {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(p).unwrap().ino()
}

#[cfg(not(target_family = "unix"))]
fn inode(_: &PathBuf) -> fake_inode::Inode {
    // since these never compare we should stop at MAX_DEPTH instead
    fake_inode::Inode{}
}

mod fake_inode {
    #[derive(Debug, Copy, Clone)]
    pub(super) struct Inode {
    }

    impl PartialEq for Inode {
        // never equal
        fn eq(&self, _other: &Self) -> bool {
            false
        }
    }

    #[cfg(test)]
    mod tests {

        #[test]
        fn fake_inode() {
            use super::Inode;
            let i = Inode{};
            let j = Inode{};
            assert_ne!(i, j);
            assert_ne!(i, i);
            assert_ne!(j, i);
        }
    }
}


// Ensure we don't recurse forever
const MAX_DEPTH: usize = 128;

/// Locate the `.upbuild` file relative to  the given path (as string)
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
