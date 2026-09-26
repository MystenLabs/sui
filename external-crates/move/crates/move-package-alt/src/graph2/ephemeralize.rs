use std::collections::BTreeMap;

use crate::{
    MoveFlavor,
    graph2::PackageGraph,
    schema::{EphemeralDependencyInfo, Publication},
};

impl<F: MoveFlavor> PackageGraph<'_, F> {
    pub fn make_ephemeral(
        self,
        _overrides: BTreeMap<EphemeralDependencyInfo, Publication<F>>,
    ) -> Self {
        todo!()
    }
}
