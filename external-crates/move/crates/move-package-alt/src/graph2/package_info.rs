use std::collections::BTreeMap;

use move_compiler::editions::Edition;

use crate::{
    MoveFlavor, NamedAddress,
    dependency::PinnedDependencyInfo,
    package::Package,
    schema::{OriginalID, PackageID, PackageName, PublishAddresses},
};

pub struct PackageInfo<'graph, F: MoveFlavor> {
    package: Package<F>,
    children: Vec<(PinnedDependencyInfo, &'graph PackageInfo<'graph, F>)>,
    parent: Option<&'graph PackageInfo<'graph, F>>,
}

impl<'graph, F: MoveFlavor> PackageInfo<'graph, F> {
    pub fn name(&self) -> &PackageName {
        todo!()
    }

    pub fn display_name(&self) -> &str {
        todo!()
    }

    pub fn display_path(&self) -> String {
        todo!()
    }

    pub fn id(&self) -> &PackageID {
        todo!()
    }

    pub fn edition(&self) -> &Option<Edition> {
        todo!()
    }

    pub fn published(&self) -> Option<PublishAddresses> {
        todo!()
    }

    pub fn is_root(&self) -> bool {
        todo!()
    }

    pub fn named_addresses(&self) -> BTreeMap<PackageName, NamedAddress> {
        todo!()
    }

    pub fn named_address(&self) -> NamedAddress {
        todo!()
    }

    pub(crate) fn original_id(&self) -> OriginalID {
        todo!()
    }

    pub(crate) fn package(&self) -> &Package<F> {
        todo!()
    }
}
