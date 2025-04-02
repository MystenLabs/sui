use std::path::Path;

pub mod lockfile;
pub mod manifest;
pub mod package;

pub use package::Package;

pub type EnvironmentName = String;
pub type PackageName = String;
