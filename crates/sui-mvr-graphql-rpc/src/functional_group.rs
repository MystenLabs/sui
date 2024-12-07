// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use async_graphql::*;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json as json;

/// Groups of features served by the RPC service.  The GraphQL Service can be configured to enable
/// or disable these features.
#[derive(Enum, Copy, Clone, Serialize, Deserialize, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "kebab-case")]
#[graphql(name = "Feature")]
pub enum FunctionalGroup {
    /// Statistics about how the network was running (TPS, top packages, APY, etc)
    Analytics,

    /// Coin metadata, per-address coin and balance information.
    Coins,

    /// Querying an object's dynamic fields.
    DynamicFields,

    /// SuiNS name and reverse name look-up.
    NameService,

    /// Transaction and Event subscriptions.
    Subscriptions,

    /// Aspects that affect the running of the system that are managed by the
    /// validators either directly, or through system transactions.
    SystemState,

    /// Named packages service (utilizing dotmove package registry).
    MoveRegistry,
}

impl FunctionalGroup {
    /// Name that the group is referred to by in configuration and responses on the GraphQL API.
    /// Not a suitable `Display` implementation because it enquotes the representation.
    pub(crate) fn name(&self) -> String {
        json::ser::to_string(self).expect("Serializing `FunctionalGroup` cannot fail.")
    }

    /// List of all functional groups
    pub(crate) fn all() -> &'static [FunctionalGroup] {
        use FunctionalGroup as G;
        static ALL: &[FunctionalGroup] = &[
            G::Analytics,
            G::Coins,
            G::DynamicFields,
            G::NameService,
            G::Subscriptions,
            G::SystemState,
            G::MoveRegistry,
        ];
        ALL
    }
}

/// Mapping from type and field name in the schema to the functional group it belongs to.
fn functional_groups() -> &'static BTreeMap<(&'static str, &'static str), FunctionalGroup> {
    // TODO: Introduce a macro to declare the functional group for a field and/or type on the
    // appropriate type, field, or function, instead of here.  This may also be able to set the
    // graphql `visible` attribute to control schema visibility by functional groups.

    use FunctionalGroup as G;
    static GROUPS: Lazy<BTreeMap<(&str, &str), FunctionalGroup>> = Lazy::new(|| {
        BTreeMap::from_iter([
            (("Address", "balance"), G::Coins),
            (("Address", "balances"), G::Coins),
            (("Address", "coins"), G::Coins),
            (("Address", "defaultSuinsName"), G::NameService),
            (("Address", "suinsRegistrations"), G::NameService),
            (("Checkpoint", "addressMetrics"), G::Analytics),
            (("Checkpoint", "networkTotalTransactions"), G::Analytics),
            (("Epoch", "protocolConfigs"), G::SystemState),
            (("Epoch", "referenceGasPrice"), G::SystemState),
            (("Epoch", "validatorSet"), G::SystemState),
            (("Object", "balance"), G::Coins),
            (("Object", "balances"), G::Coins),
            (("Object", "coins"), G::Coins),
            (("Object", "defaultSuinsName"), G::NameService),
            (("Object", "dynamicField"), G::DynamicFields),
            (("Object", "dynamicObjectField"), G::DynamicFields),
            (("Object", "dynamicFields"), G::DynamicFields),
            (("Object", "suinsRegistrations"), G::NameService),
            (("Owner", "balance"), G::Coins),
            (("Owner", "balances"), G::Coins),
            (("Owner", "coins"), G::Coins),
            (("Owner", "defaultSuinsName"), G::NameService),
            (("Owner", "dynamicField"), G::DynamicFields),
            (("Owner", "dynamicObjectField"), G::DynamicFields),
            (("Owner", "dynamicFields"), G::DynamicFields),
            (("Owner", "suinsRegistrations"), G::NameService),
            (("Query", "coinMetadata"), G::Coins),
            (("Query", "moveCallMetrics"), G::Analytics),
            (("Query", "networkMetrics"), G::Analytics),
            (("Query", "protocolConfig"), G::SystemState),
            (("Query", "resolveSuinsAddress"), G::NameService),
            (("Query", "packageByName"), G::MoveRegistry),
            (("Query", "typeByName"), G::MoveRegistry),
            (("Subscription", "events"), G::Subscriptions),
            (("Subscription", "transactions"), G::Subscriptions),
            (("SystemStateSummary", "safeMode"), G::SystemState),
            (("SystemStateSummary", "storageFund"), G::SystemState),
            (("SystemStateSummary", "systemParameters"), G::SystemState),
            (("SystemStateSummary", "systemStateVersion"), G::SystemState),
        ])
    });

    Lazy::force(&GROUPS)
}

/// Map a type and field name to a functional group.  If an explicit group does not exist for the
/// field, then it is assumed to be a "core" feature.
pub(crate) fn functional_group(type_: &str, field: &str) -> Option<FunctionalGroup> {
    functional_groups().get(&(type_, field)).copied()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use async_graphql::registry::Registry;
    use async_graphql::OutputType;

    use crate::types::query::Query;

    use super::*;

    #[test]
    /// Makes sure all the functional groups correspond to real elements of the schema unless they
    /// are explicitly recorded as unimplemented.  Complementarily, makes sure that fields marked as
    /// unimplemented don't appear in the set of unimplemented fields.
    fn test_groups_match_schema() {
        let mut registry = Registry::default();
        Query::create_type_info(&mut registry);

        let unimplemented = BTreeSet::from_iter([
            ("Checkpoint", "addressMetrics"),
            ("Epoch", "protocolConfig"),
            ("Query", "moveCallMetrics"),
            ("Query", "networkMetrics"),
            ("Subscription", "events"),
            ("Subscription", "transactions"),
        ]);

        for (type_, field) in &unimplemented {
            let Some(meta_type) = registry.concrete_type_by_name(type_) else {
                continue;
            };

            let Some(_) = meta_type.field_by_name(field) else {
                continue;
            };

            panic!(
                "Field '{type_}.{field}' is marked as unimplemented in this test, but it's in the \
                 schema.  Fix this by removing it from the `unimplemented` set."
            );
        }

        for (type_, field) in functional_groups().keys() {
            if unimplemented.contains(&(type_, field)) {
                continue;
            }

            let Some(meta_type) = registry.concrete_type_by_name(type_) else {
                panic!("Type '{type_}' from functional group configs does not appear in schema.");
            };

            let Some(_) = meta_type.field_by_name(field) else {
                panic!(
                    "Field '{type_}.{field}' from functional group configs does not appear in \
                     schema."
                );
            };
        }
    }
}
