// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    flavor::MoveFlavor,
    schema::{PackageID, Pin},
};
use std::collections::BTreeMap;

use super::PackageGraph;

impl<F: MoveFlavor> From<&PackageGraph<F>> for BTreeMap<PackageID, Pin> {
    /// Convert a PackageGraph into an entry in the lockfile's `[pinned]` section.
    fn from(value: &PackageGraph<F>) -> Self {
        let mut result = Self::new();

        for (id, pkg) in value.all_packages() {
            let deps = pkg
                .direct_deps()
                .iter()
                .map(|(name, pkg)| (name.clone(), pkg.id().clone()))
                .collect();

            let pin = Pin {
                source: pkg.dep_for_self().clone().into(),
                address_override: None, // TODO: this needs to be stored in the package node
                use_environment: Some(pkg.package().environment_name().clone()),
                manifest_digest: pkg.package().digest().to_string(),
                deps,
            };

            result.insert(id.clone(), pin);
        }

        result
    }
}
