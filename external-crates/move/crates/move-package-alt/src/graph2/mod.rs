/// defines [PackageGraph::make_ephemeral]
mod ephemeralize;

/// defines [PackageGraph::apply_linkage]
mod linkage;

/// defines [PackageGraph::load]
mod loader;

/// defines [PackageGraph::filter_for_mode]
mod modes;

/// defines [PackageGraph::to_lockfile]
mod to_lockfile;

mod package_info;
pub use package_info::PackageInfo;

use std::{collections::BTreeMap, path::PathBuf};

use crate::MoveFlavor;

pub struct PackageGraph<'graph, F: MoveFlavor> {
    all_packages: BTreeMap<PathBuf, PackageInfo<'graph, F>>,
    root: &'graph PackageInfo<'graph, F>,
}

impl<F: MoveFlavor> PackageGraph<'_, F> {
    pub fn root(&self) -> &PackageInfo<F> {
        self.root
    }

    pub fn packages(&self) -> impl Iterator<Item = &PackageInfo<F>> {
        self.all_packages.values()
    }
}
