// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use petgraph::visit::{EdgeRef, IntoNodeReferences};

use crate::{
    errors::PackageResult,
    flavor::MoveFlavor,
    schema::{PackageID, Pin},
};
use std::collections::BTreeMap;

use super::PackageGraph;

#[allow(unused)]
impl<F: MoveFlavor> PackageGraph<F> {
    /// Output the pins corresponding to this package graph. Fails if some package has multiple
    /// dependencies with the same name (this is unsupported right now, but will be enabled with
    /// modes).
    pub fn to_pins(&self) -> PackageResult<BTreeMap<PackageID, Pin>> {
        let mut result = BTreeMap::new();

        for (node, pkg) in self.inner.node_references() {
            let mut deps = BTreeMap::new();

            for edge in self.inner.edges(node) {
                let dep_name = edge.weight().name().clone();
                let target_id = self
                    .package_ids
                    .get_by_right(&edge.target())
                    .expect("all nodes are in package_ids")
                    .clone();

                let old = deps.insert(dep_name, target_id);

                if old.is_some() {
                    // this indicates that there are multiple dependencies with the same name in a
                    // package. This is currently impossible but may become possible when modes are
                    // implemented
                    todo!()
                }
            }

            let pin = Pin {
                source: pkg.dep_for_self().clone().into(),
                address_override: None, // TODO: this needs to be stored in the package node
                use_environment: Some(pkg.environment_name().clone()),
                manifest_digest: pkg.digest().to_string(),
                deps,
            };

            let id = self
                .package_ids
                .get_by_right(&node)
                .expect("all nodes are in package_ids")
                .clone();

            result.insert(id, pin);
        }

        Ok(result)
    }
}
