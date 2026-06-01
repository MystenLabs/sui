// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tree shaking: computing the publishable linkage table.
//!
//! The linkage table we publish must satisfy the on-chain publish check
//! (`MovePackage::build_linkage_table` in `sui-types`): it must contain the full on-chain
//! linkage table of every dependency it retains.
//!
//! We compute it as the closure of the root's *directly used* dependencies (the bytecode
//! seed) over the on-chain linkage tables of the root's *declared direct* dependencies —
//! and of those only. We never fetch a transitive dependency's linkage table.
//!
//! That this is sufficient relies on an invariant enforced by `move-package-alt`'s
//! linkage check (`graph/linkage.rs`): a version conflict can only be resolved by an
//! *override* dependency declared on every root-to-package path, and an override is
//! always a *direct* dependency edge. Therefore:
//!
//!   - A conflict confined to one direct dependency `d`'s subtree is resolved by
//!     overrides within that subtree. `d` was itself published with those overrides,
//!     so `d`'s on-chain (flat) linkage table already lists its whole subtree at the
//!     final resolved versions, each with its complete closure.
//!
//!   - A conflict spanning two of the root's direct subtrees cannot be resolved within
//!     either; the override is forced up to their common ancestor — the root — which
//!     makes the conflicted package a *direct* dependency of the root.
//!
//! So every package in the final linkage table is either a direct dependency (fetched
//! directly) or lies in some direct dependency's subtree at its final version (carried
//! by that dependency's flat table). The closure therefore needs no transitive fetches
//! and no fixpoint over the kept set.
//!
//! The seed is `bytecode-direct` rather than "all direct dependencies" so that unused
//! direct dependencies — implicit system packages, version-pinning-only deps — are
//! still shaken out: they enter `required` only if something reachable actually
//! references them.
//!
//! This assumes each dependency's source is consistent with its on-chain publication;
//! an inconsistency is caught by the best-effort checks below or, failing that,
//! rejected by the chain.
//!
//! CAUTION: correctness depends on the override invariant above. If `move-package-alt`
//! ever lets a transitive package be force-upgraded without an override reaching the
//! root, this closure will silently drop linkage entries. Keep in sync with
//! `graph/linkage.rs`.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, anyhow, bail};
use move_core_types::account_address::AccountAddress;
use sui_move_build::{CompiledPackage, OriginalID, PublishedDep, PublishedID};
use sui_rpc_api::Client;
use sui_types::{base_types::ObjectID, move_package::UpgradeInfo};

/// The original IDs referenced directly by the modules being published — the
/// `immediate_dependencies` of the considered modules, minus those modules' own
/// addresses. With `with_unpublished_deps`, the considered modules are all modules at
/// address `0x0`; otherwise they are the root package's modules.
fn direct_dep_original_ids(
    with_unpublished_deps: bool,
    compiled_package: &CompiledPackage,
) -> BTreeSet<OriginalID> {
    let considered: Vec<_> = if with_unpublished_deps {
        compiled_package
            .package
            .all_compiled_units_with_source()
            .filter(|m| m.unit.address.into_inner() == AccountAddress::ZERO)
            .collect()
    } else {
        compiled_package.package.root_modules().collect()
    };

    let self_addresses: BTreeSet<AccountAddress> = considered
        .iter()
        .map(|m| *m.unit.module.self_id().address())
        .collect();

    considered
        .iter()
        .flat_map(|m| m.unit.module.immediate_dependencies())
        .map(|dep| *dep.address())
        .filter(|addr| !self_addresses.contains(addr))
        .map(OriginalID)
        .collect()
}

/// Fetch the on-chain linkage table of the package published at `pkg_id`: each
/// transitive dependency's original ID mapped to its on-chain published ID and version.
/// Fails on RPC error or if `pkg_id` is not a package.
async fn onchain_linkage(
    client: &mut Client,
    pkg_id: PublishedID,
) -> anyhow::Result<BTreeMap<OriginalID, UpgradeInfo>> {
    let object_id = ObjectID::from_address(pkg_id.0);
    let object = client
        .get_object(object_id)
        .await
        .map_err(|e| anyhow!("{}", e.message()))?;
    let package = object
        .data
        .try_as_package()
        .ok_or_else(|| anyhow!("object at {object_id} is not a package"))?;
    Ok(package
        .linkage_table()
        .iter()
        .map(|(original_id, info)| (OriginalID(AccountAddress::from(*original_id)), info.clone()))
        .collect())
}

/// Compute the set of original IDs that the linkage table of `compiled_package` must
/// contain when published, performing best-effort validation as it goes.
///
/// Bails if:
///   - the bytecode references an original ID that is not a resolved dependency (A1);
///   - a direct dependency's on-chain linkage table references a package that is not a
///     resolved dependency (A2);
///   - the resolved version of a transitive dependency is older than a direct
///     dependency's on-chain linkage table requires (B).
fn compute_required_linkage(
    seed: &BTreeSet<OriginalID>,
    candidates: &BTreeMap<OriginalID, PublishedDep>,
    direct_linkages: &BTreeMap<OriginalID, BTreeMap<OriginalID, UpgradeInfo>>,
) -> anyhow::Result<BTreeSet<OriginalID>> {
    // A1: every directly-referenced package must be a resolved dependency. The compiler
    // only emits references to declared dependencies, so this is a defensive check.
    for oid in seed {
        if !candidates.contains_key(oid) {
            bail!("the package references {oid}, which is not one of its resolved dependencies");
        }
    }

    let mut required = BTreeSet::new();
    let mut worklist: Vec<OriginalID> = seed.iter().copied().collect();

    while let Some(oid) = worklist.pop() {
        if !required.insert(oid) {
            continue;
        }
        // Expand `oid` only if it is a direct dependency, i.e. we hold its linkage table.
        let Some(linkage) = direct_linkages.get(&oid) else {
            continue;
        };
        // `oid` has a fetched linkage table, so it is a direct dependency and a candidate.
        let x = &candidates[&oid];
        for (y, info) in linkage {
            match candidates.get(y) {
                None => bail!(
                    "package {x_name} depends on on-chain package {y_published}, but the \
                     source for {x_name} does not depend on that package. This likely \
                     indicates a mismatch between the source package and the on-chain \
                     bytecode for {x_name}.",
                    x_name = x.name,
                    y_published = info.upgraded_id,
                ),
                Some(y_dep) => {
                    if y_dep.version < info.upgraded_version.value() {
                        bail!(
                            "on-chain, package {x_name} depends on version {n} of {y_name}, \
                             but the source build resolves {y_name} to the older version {m}. \
                             This likely indicates a mismatch between the source packages and \
                             what is published on-chain.",
                            x_name = x.name,
                            n = info.upgraded_version.value(),
                            y_name = y_dep.name,
                            m = y_dep.version,
                        );
                    }
                }
            }
            worklist.push(*y);
        }
    }

    Ok(required)
}

/// Compute the set of original IDs that the published linkage table must contain. Reads
/// the bytecode for the seed, then fetches the on-chain linkage table of each declared
/// direct dependency before handing everything to [`compute_required_linkage`].
async fn required_linkage_oids(
    client: &mut Client,
    compiled_package: &CompiledPackage,
    with_unpublished_deps: bool,
) -> anyhow::Result<BTreeSet<OriginalID>> {
    let seed = direct_dep_original_ids(with_unpublished_deps, compiled_package);
    let candidates = &compiled_package.dependency_ids.published;

    let mut direct_linkages = BTreeMap::new();
    for (oid, dep) in candidates {
        if !dep.is_direct {
            continue;
        }
        let linkage = onchain_linkage(client, dep.published_id)
            .await
            .with_context(|| format!("Failed to fetch package {}", dep.name))?;
        direct_linkages.insert(*oid, linkage);
    }

    compute_required_linkage(&seed, candidates, &direct_linkages)
}

/// Filter `compiled_package.dependency_ids.published` to the dependencies that must
/// appear in the published linkage table. Implicit and otherwise unused dependencies
/// are dropped; dependencies pinned by a kept dependency's on-chain linkage table are
/// retained.
pub(crate) async fn pkg_tree_shake(
    mut client: Client,
    with_unpublished_deps: bool,
    compiled_package: &mut CompiledPackage,
) -> anyhow::Result<()> {
    let required =
        required_linkage_oids(&mut client, compiled_package, with_unpublished_deps).await?;
    compiled_package
        .dependency_ids
        .published
        .retain(|oid, _| required.contains(oid));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_symbol_pool::Symbol;
    use sui_types::base_types::SequenceNumber;

    fn oid(n: u16) -> OriginalID {
        OriginalID::from(n)
    }

    fn pid(n: u16) -> PublishedID {
        PublishedID::from(n)
    }

    fn dep(name: &str, published_id: PublishedID, version: u64, is_direct: bool) -> PublishedDep {
        PublishedDep {
            published_id,
            version,
            is_direct,
            name: Symbol::from(name),
        }
    }

    fn info(published_id: PublishedID, version: u64) -> UpgradeInfo {
        UpgradeInfo {
            upgraded_id: ObjectID::from_address(published_id.0),
            upgraded_version: SequenceNumber::from_u64(version),
        }
    }

    /// Basic shake: A is bytecode-direct and a direct dep; B is a declared but unused
    /// implicit dep. `required` keeps A, drops B.
    #[test]
    fn shakes_unused_direct_dep() {
        let seed = BTreeSet::from([oid(1)]);
        let candidates = BTreeMap::from([
            (oid(1), dep("a", pid(1), 1, true)),
            (oid(2), dep("b", pid(2), 1, true)), // unused implicit dep
        ]);
        let direct_linkages =
            BTreeMap::from([(oid(1), BTreeMap::new()), (oid(2), BTreeMap::new())]);

        let required = compute_required_linkage(&seed, &candidates, &direct_linkages).unwrap();
        assert_eq!(required, BTreeSet::from([oid(1)]));
    }

    /// Cruft retained: A is bytecode-direct; A's on-chain linkage table mentions Z (an
    /// unused implicit dep). Z must be retained because chain rule 2 requires it.
    #[test]
    fn retains_pre_tree_shake_cruft() {
        let seed = BTreeSet::from([oid(1)]);
        let candidates = BTreeMap::from([
            (oid(1), dep("a", pid(1), 1, true)),
            (oid(9), dep("z", pid(9), 1, true)),
        ]);
        let direct_linkages = BTreeMap::from([
            (oid(1), BTreeMap::from([(oid(9), info(pid(9), 1))])),
            (oid(9), BTreeMap::new()),
        ]);

        let required = compute_required_linkage(&seed, &candidates, &direct_linkages).unwrap();
        assert_eq!(required, BTreeSet::from([oid(1), oid(9)]));
    }

    /// Forced version pulls in a new transitive dep: C is pinned by R (declared direct,
    /// not bytecode-used); A links C@1; C@2 (the pinned version) links D. The closure
    /// must reach D via C@2's direct linkage table.
    #[test]
    fn forced_version_pulls_new_transitive() {
        let seed = BTreeSet::from([oid(1)]); // root uses A
        let candidates = BTreeMap::from([
            (oid(1), dep("a", pid(1), 1, true)),
            (oid(3), dep("c", pid(32), 2, true)), // C resolved to version 2 (pinned)
            (oid(4), dep("d", pid(4), 1, true)),  // D — new dep introduced by C@2
        ]);
        // A links C@1 (which has no transitive deps in this fixture); C@2 links D.
        let direct_linkages = BTreeMap::from([
            (oid(1), BTreeMap::from([(oid(3), info(pid(31), 1))])),
            (oid(3), BTreeMap::from([(oid(4), info(pid(4), 1))])),
            (oid(4), BTreeMap::new()),
        ]);

        let required = compute_required_linkage(&seed, &candidates, &direct_linkages).unwrap();
        assert_eq!(required, BTreeSet::from([oid(1), oid(3), oid(4)]));
    }

    /// A2 fires: A's on-chain linkage table mentions Y, but Y is not a candidate.
    #[test]
    fn a2_missing_candidate() {
        let seed = BTreeSet::from([oid(1)]);
        let candidates = BTreeMap::from([(oid(1), dep("a", pid(1), 1, true))]);
        let direct_linkages =
            BTreeMap::from([(oid(1), BTreeMap::from([(oid(7), info(pid(7), 1))]))]);

        let err = compute_required_linkage(&seed, &candidates, &direct_linkages).unwrap_err();
        assert!(err.to_string().contains("does not depend on that package"));
    }

    /// B fires: A's linkage requires C@version 5, but the resolved C is version 2.
    #[test]
    fn b_version_downgrade() {
        let seed = BTreeSet::from([oid(1)]);
        let candidates = BTreeMap::from([
            (oid(1), dep("a", pid(1), 1, true)),
            (oid(3), dep("c", pid(32), 2, true)), // resolved to version 2
        ]);
        let direct_linkages = BTreeMap::from([
            (oid(1), BTreeMap::from([(oid(3), info(pid(35), 5))])), // needs version 5
        ]);

        let err = compute_required_linkage(&seed, &candidates, &direct_linkages).unwrap_err();
        assert!(err.to_string().contains("older version"));
    }
}
