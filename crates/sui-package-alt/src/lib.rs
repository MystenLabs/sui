#![allow(unused)]

use std::collections::BTreeMap;

use move_package_alt::{
    dependency::{self, Pinned, Unpinned},
    errors::PackageResult,
    flavor::MoveFlavor,
    package::PackageName,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename = "kebab-case")]
struct OnChainDependency {
    on_chain: bool,
}

struct SuiFlavor;

impl MoveFlavor for SuiFlavor {
    type FlavorDependency<P: ?Sized> = OnChainDependency;

    fn pin(
        &self,
        deps: BTreeMap<PackageName, Self::FlavorDependency<Unpinned>>,
    ) -> PackageResult<BTreeMap<PackageName, Self::FlavorDependency<Pinned>>> {
        todo!()
    }

    fn fetch(
        &self,
        deps: BTreeMap<PackageName, Self::FlavorDependency<Pinned>>,
    ) -> PackageResult<BTreeMap<PackageName, std::path::PathBuf>> {
        todo!()
    }

    type PublishedMetadata = (); // TODO

    type EnvironmentID = (); // TODO

    fn implicit_dependencies(
        &self,
        id: Self::EnvironmentID,
    ) -> Vec<dependency::PinnedDependency<Self>> {
        todo!()
    }
}
