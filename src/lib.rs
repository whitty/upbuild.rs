// SPDX-License-Identifier: GPL-3.0-or-later
// (C) Copyright 2024 Greg Whiteley
//
//! `upbuild_rs` is a rust reimplementation of my hacky integration
//! helper `upbuild` as seen [here](https://github.com/whitty/upbuild).

#![warn(missing_docs)]

mod error;
mod file;
mod exec;
mod find;
mod cfg;

pub use file::ClassicFile;

pub use exec::Exec;
pub use exec::process_runner;
pub use exec::print_runner;

pub use find::find;
pub use cfg::Config;

/// The Error type for this tool
pub type Error = error::Error;
/// Bind the implied Error type for convenience
pub type Result<T> = std::result::Result<T, Error>;
