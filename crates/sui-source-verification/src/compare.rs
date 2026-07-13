// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use futures::future;
use move_binary_format::CompiledModule;
use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;
use sui_rpc_api::Client;
use sui_types::base_types::ObjectID;

use crate::build::GeneratedPackage;
use crate::error::{AggregateError, Error};
use crate::onchain::{OnChainPackage, pkg_for_address};

/// Compare a freshly-built package against its on-chain representation: module bytecode (after
/// rewriting the source's `0x0` self-address to the on-chain original id) and linkage. Collects all
/// discrepancies into an [`AggregateError`].
pub async fn check(
    client: &Client,
    generated: GeneratedPackage,
    onchain: OnChainPackage,
) -> Result<(), AggregateError> {
    let mut errs = vec![];

    let generated_modules = rewrite_modules(generated.modules, onchain.original_id, &mut errs);
    compare_modules(generated_modules, onchain.modules, &mut errs);
    compare_linkage(client, &generated.dependencies, &onchain.linkage, &mut errs).await;

    if errs.is_empty() {
        Ok(())
    } else {
        Err(AggregateError(errs))
    }
}

/// Rewrite each generated module's `0x0` self-address to `original_id`, keying the results by module
/// name. A module already at `original_id` is kept as-is (some older toolchains emit the published
/// address rather than `0x0`); any other non-zero self-address is reported as an error.
fn rewrite_modules(
    modules: Vec<CompiledModule>,
    original_id: AccountAddress,
    errs: &mut Vec<Error>,
) -> BTreeMap<Symbol, CompiledModule> {
    let mut out = BTreeMap::new();
    for module in modules {
        let name = Symbol::from(module.self_id().name().as_str());
        match substitute_root_address(module, original_id) {
            Ok(m) => {
                out.insert(name, m);
            }
            Err(e) => errs.push(e),
        }
    }
    out
}

/// Require the generated and on-chain module sets to be identical and byte-for-byte equal.
fn compare_modules(
    generated: BTreeMap<Symbol, CompiledModule>,
    mut onchain: BTreeMap<Symbol, CompiledModule>,
    errs: &mut Vec<Error>,
) {
    for (name, generated_module) in generated {
        match onchain.remove(&name) {
            Some(onchain_module) if generated_module == onchain_module => {}
            Some(_) => errs.push(Error::ModuleBytecodeMismatch { module: name }),
            None => errs.push(Error::SourceModuleNotOnChain { module: name }),
        }
    }
    for name in onchain.into_keys() {
        errs.push(Error::OnChainModuleNotInSource { module: name });
    }
}

/// Require that every dependency shared by both linkages resolves to the same storage id. The
/// modules are compared separately and must match, so the two packages reference the same
/// dependencies; a dependency that appears in only one linkage is therefore one the code does not
/// reference (tree-shaking kept it on one side but not the other) and is ignored. A shared
/// dependency at differing storage ids is a real version mismatch.
async fn compare_linkage(
    client: &Client,
    generated: &[ObjectID],
    onchain: &BTreeMap<AccountAddress, AccountAddress>,
    errs: &mut Vec<Error>,
) {
    let generated = fetch_generated_linkage(client, generated, errs).await;
    diff_linkage(&generated, onchain, errs);
}

/// Compare two `original id -> storage id` linkage maps: a shared original at differing storage
/// ids is a mismatch; an original present in only one map is ignored (unused).
fn diff_linkage(
    generated: &BTreeMap<AccountAddress, AccountAddress>,
    onchain: &BTreeMap<AccountAddress, AccountAddress>,
    errs: &mut Vec<Error>,
) {
    for (original, on_chain_storage) in onchain {
        if let Some(source_storage) = generated.get(original)
            && source_storage != on_chain_storage
        {
            errs.push(Error::LinkageVersionMismatch {
                original: *original,
                on_chain: *on_chain_storage,
                in_source: *source_storage,
            });
        }
    }
}

/// Resolve the generated linkage's storage ids to `original id -> storage id` by fetching each
/// dependency package. A storage id that cannot be fetched is reported as
/// [`Error::SourceDependencyNotOnChain`].
async fn fetch_generated_linkage(
    client: &Client,
    deps: &[ObjectID],
    errs: &mut Vec<Error>,
) -> BTreeMap<AccountAddress, AccountAddress> {
    let results = future::join_all(
        deps.iter()
            .map(|id| async move { (*id, pkg_for_address(client, *id).await) }),
    )
    .await;
    let mut map = BTreeMap::new();
    for (storage_id, result) in results {
        match result {
            Ok(pkg) => {
                map.insert(pkg.original_package_id().into(), storage_id.into());
            }
            Err(_) => errs.push(Error::SourceDependencyNotOnChain(storage_id.into())),
        }
    }
    map
}

/// Return a copy of `module` with its `0x0` self-address replaced by `root`. Older toolchains emit
/// an already-published package at its on-chain address rather than at `0x0`, which is accepted
/// unchanged. Any other self-address is an error.
fn substitute_root_address(
    mut module: CompiledModule,
    root: AccountAddress,
) -> Result<CompiledModule, Error> {
    let name = module.self_id().name().to_string();
    let address_idx = module.self_handle().address;

    let Some(addr) = module.address_identifiers.get_mut(address_idx.0 as usize) else {
        return Err(Error::InvalidModule {
            name,
            message: "self address field missing".into(),
        });
    };

    if *addr == root {
        return Ok(module);
    }

    if *addr != AccountAddress::ZERO {
        return Err(Error::InvalidModule {
            name,
            message: "self address already populated".into(),
        });
    }

    *addr = root;
    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_binary_format::file_format::{basic_test_module, empty_module};

    fn addr(byte: u8) -> AccountAddress {
        let mut bytes = [0u8; AccountAddress::LENGTH];
        bytes[AccountAddress::LENGTH - 1] = byte;
        AccountAddress::new(bytes)
    }

    fn sym(s: &str) -> Symbol {
        Symbol::from(s)
    }

    #[test]
    fn substitute_rewrites_zero_self_address() {
        // empty_module()'s self-address is 0x0.
        let rewritten = substitute_root_address(empty_module(), addr(7)).unwrap();
        assert_eq!(rewritten.self_id().address(), &addr(7));
    }

    #[test]
    fn substitute_rejects_populated_self_address() {
        let already = substitute_root_address(empty_module(), addr(7)).unwrap();
        // A module whose self-address is no longer 0x0 cannot be rewritten again.
        assert!(matches!(
            substitute_root_address(already, addr(9)),
            Err(Error::InvalidModule { .. })
        ));
    }

    #[test]
    fn compare_modules_accepts_identical_sets() {
        let generated = BTreeMap::from([(sym("a"), empty_module())]);
        let onchain = BTreeMap::from([(sym("a"), empty_module())]);
        let mut errs = vec![];
        compare_modules(generated, onchain, &mut errs);
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn compare_modules_flags_bytecode_mismatch() {
        let generated = BTreeMap::from([(sym("a"), empty_module())]);
        let onchain = BTreeMap::from([(sym("a"), basic_test_module())]);
        let mut errs = vec![];
        compare_modules(generated, onchain, &mut errs);
        assert!(matches!(
            errs.as_slice(),
            [Error::ModuleBytecodeMismatch { module }] if module.as_str() == "a"
        ));
    }

    #[test]
    fn compare_modules_flags_missing_and_extra() {
        let generated = BTreeMap::from([(sym("a"), empty_module()), (sym("b"), empty_module())]);
        let onchain = BTreeMap::from([(sym("a"), empty_module()), (sym("c"), empty_module())]);
        let mut errs = vec![];
        compare_modules(generated, onchain, &mut errs);
        // `b` is source-only, `c` is on-chain-only.
        assert!(errs.iter().any(
            |e| matches!(e, Error::SourceModuleNotOnChain { module } if module.as_str() == "b")
        ));
        assert!(errs.iter().any(
            |e| matches!(e, Error::OnChainModuleNotInSource { module } if module.as_str() == "c")
        ));
        assert_eq!(errs.len(), 2);
    }

    #[test]
    fn diff_linkage_accepts_matching_and_ignores_one_sided() {
        // Shared dep at the same version; plus one generated-only and one on-chain-only dep.
        let generated = BTreeMap::from([(addr(1), addr(10)), (addr(2), addr(20))]);
        let onchain = BTreeMap::from([(addr(1), addr(10)), (addr(3), addr(30))]);
        let mut errs = vec![];
        diff_linkage(&generated, &onchain, &mut errs);
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn diff_linkage_flags_version_mismatch() {
        // Same original dep (0x1), different storage version.
        let generated = BTreeMap::from([(addr(1), addr(11))]);
        let onchain = BTreeMap::from([(addr(1), addr(10))]);
        let mut errs = vec![];
        diff_linkage(&generated, &onchain, &mut errs);
        assert!(matches!(
            errs.as_slice(),
            [Error::LinkageVersionMismatch { original, on_chain, in_source }]
                if *original == addr(1) && *on_chain == addr(10) && *in_source == addr(11)
        ));
    }
}
