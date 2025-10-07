// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module contains the implementation of the symbolication runner, which is responsible
//! for coordinating the symbolication process between different threads
//! in the analyzer.

use crate::symbols::{
    Symbols,
    compilation::{CachedPackages, MANIFEST_FILE_NAME},
    get_symbols,
};

use anyhow::{Result, anyhow};
use crossbeam::channel::Sender;
use lsp_types::Diagnostic;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
};
use vfs::VfsPath;

use move_compiler::{editions::Flavor, linters::LintLevel};
use move_package::source_package::parsed_manifest::Dependencies;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum RunnerState {
    Run(BTreeSet<PathBuf>),
    Wait,
    Quit,
}

/// Data used during symbolication running and symbolication info updating
pub struct SymbolicatorRunner {
    mtx_cvar: Arc<(Mutex<RunnerState>, Condvar)>,
}

impl SymbolicatorRunner {
    /// Create a new idle runner (one that does not actually symbolicate)
    pub fn idle() -> Self {
        let mtx_cvar = Arc::new((Mutex::new(RunnerState::Wait), Condvar::new()));
        SymbolicatorRunner { mtx_cvar }
    }

    /// Create a new runner
    pub fn new(
        ide_files_root: VfsPath,
        symbols_map: Arc<Mutex<BTreeMap<PathBuf, Symbols>>>,
        packages_info: Arc<Mutex<CachedPackages>>,
        sender: Sender<Result<BTreeMap<PathBuf, Vec<Diagnostic>>>>,
        lint: LintLevel,
        implicit_deps: Dependencies,
        flavor: Option<Flavor>,
    ) -> Self {
        let mtx_cvar = Arc::new((Mutex::new(RunnerState::Wait), Condvar::new()));
        let thread_mtx_cvar = mtx_cvar.clone();
        let runner = SymbolicatorRunner { mtx_cvar };

        thread::Builder::new()
            .spawn(move || {
                let (mtx, cvar) = &*thread_mtx_cvar;
                // Locations opened in the IDE (files or directories) for which manifest file is missing
                let mut missing_manifests = BTreeSet::new();
                // infinite loop to wait for symbolication requests
                eprintln!("starting symbolicator runner loop");
                loop {
                    let starting_paths_opt = {
                        // hold the lock only as long as it takes to get the data, rather than through
                        // the whole symbolication process (hence a separate scope here)
                        let mut symbolicate = mtx.lock().unwrap();
                        match symbolicate.clone() {
                            RunnerState::Quit => break,
                            RunnerState::Run(starting_paths) => {
                                *symbolicate = RunnerState::Wait;
                                Some(starting_paths)
                            }
                            RunnerState::Wait => {
                                // wait for next request
                                symbolicate = cvar.wait(symbolicate).unwrap();
                                match symbolicate.clone() {
                                    RunnerState::Quit => break,
                                    RunnerState::Run(starting_paths) => {
                                        *symbolicate = RunnerState::Wait;
                                        Some(starting_paths)
                                    }
                                    RunnerState::Wait => None,
                                }
                            }
                        }
                    };
                    if let Some(starting_paths) = starting_paths_opt {
                        // aggregate all starting paths by package
                        let pkgs_to_analyze = Self::pkgs_to_analyze(
                            starting_paths,
                            &mut missing_manifests,
                            sender.clone(),
                        );
                        for pkg_path in pkgs_to_analyze.into_iter() {
                            eprintln!("symbolication started");
                            match get_symbols(
                                packages_info.clone(),
                                ide_files_root.clone(),
                                pkg_path.as_path(),
                                lint,
                                None,
                                implicit_deps.clone(),
                                flavor,
                            ) {
                                Ok((symbols_opt, lsp_diagnostics)) => {
                                    eprintln!("symbolication finished");
                                    if let Some(new_symbols) = symbols_opt {
                                        // replace symbolication info for a given package
                                        //
                                        // TODO: we may consider "unloading" symbolication information when
                                        // files/directories are being closed but as with other performance
                                        // optimizations (e.g. incrementalizatino of the vfs), let's wait
                                        // until we know we actually need it
                                        let mut old_symbols_map = symbols_map.lock().unwrap();
                                        old_symbols_map.insert(pkg_path.clone(), new_symbols);
                                    }
                                    // set/reset (previous) diagnostics
                                    if let Err(err) = sender.send(Ok(lsp_diagnostics)) {
                                        eprintln!("could not pass diagnostics: {:?}", err);
                                    }
                                }
                                Err(err) => {
                                    eprintln!("symbolication failed: {:?}", err);
                                    if let Err(err) = sender.send(Err(err)) {
                                        eprintln!("could not pass compiler error: {:?}", err);
                                    }
                                }
                            }
                        }
                    }
                }
            })
            .unwrap();

        runner
    }

    /// Collects all packages to compiler based on starting file paths passed as arguments
    fn pkgs_to_analyze(
        starting_paths: BTreeSet<PathBuf>,
        missing_manifests: &mut BTreeSet<PathBuf>,
        sender: Sender<Result<BTreeMap<PathBuf, Vec<Diagnostic>>>>,
    ) -> BTreeSet<PathBuf> {
        let mut pkgs_to_analyze = BTreeSet::new();
        for starting_path in &starting_paths {
            let Some(root_dir) = Self::root_dir(starting_path) else {
                if !missing_manifests.contains(starting_path) {
                    eprintln!("reporting missing manifest");
                    // report missing manifest file only once to avoid cluttering IDE's UI in
                    // cases when developer indeed intended to open a standalone file that was
                    // not meant to compile
                    missing_manifests.insert(starting_path.clone());
                    if let Err(err) = sender.send(Err(anyhow!(
                        "Unable to find package manifest. Make sure that
                    the source files are located in a sub-directory of a package containing
                    a Move.toml file. "
                    ))) {
                        eprintln!("could not pass missing manifest error: {:?}", err);
                    }
                }
                continue;
            };
            pkgs_to_analyze.insert(root_dir.clone());
        }
        pkgs_to_analyze
    }

    pub fn run(&self, starting_path: PathBuf) {
        eprintln!("scheduling run for {:?}", starting_path);
        let (mtx, cvar) = &*self.mtx_cvar;
        let mut symbolicate = mtx.lock().unwrap();
        match symbolicate.clone() {
            RunnerState::Quit => (), // do nothing as we are quitting
            RunnerState::Run(mut all_starting_paths) => {
                all_starting_paths.insert(starting_path);
                *symbolicate = RunnerState::Run(all_starting_paths);
            }
            RunnerState::Wait => {
                let mut all_starting_paths = BTreeSet::new();
                all_starting_paths.insert(starting_path);
                *symbolicate = RunnerState::Run(all_starting_paths);
            }
        }
        cvar.notify_one();
        eprintln!("scheduled run");
    }

    pub fn quit(&self) {
        let (mtx, cvar) = &*self.mtx_cvar;
        let mut symbolicate = mtx.lock().unwrap();
        *symbolicate = RunnerState::Quit;
        cvar.notify_one();
    }

    /// Finds manifest file in a (sub)directory of the starting path passed as argument
    pub fn root_dir(starting_path: &Path) -> Option<PathBuf> {
        let mut current_path_opt = Some(starting_path);
        while current_path_opt.is_some() {
            let current_path = current_path_opt.unwrap();
            let manifest_path = current_path.join(MANIFEST_FILE_NAME);
            if manifest_path.is_file() {
                return Some(current_path.to_path_buf());
            }
            current_path_opt = current_path.parent();
        }
        None
    }
}
