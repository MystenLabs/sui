use std::collections::BTreeMap;

use crate::{
    MoveFlavor,
    errors::PackageResult,
    graph2::PackageGraph,
    schema::{PackageID, Pin},
};

impl<F: MoveFlavor> PackageGraph<'_, F> {
    pub fn to_lockfile(&self) -> PackageResult<BTreeMap<PackageID, Pin>> {
        todo!()
    }
}
