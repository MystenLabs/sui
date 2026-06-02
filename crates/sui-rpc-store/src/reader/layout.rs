// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Move type-layout resolver shim.
//!
//! [`RpcStateReader::get_struct_layout_with_overlay`] needs an
//! [`Executor`] over an epoch's [`ProtocolConfig`] plus a
//! [`BackingPackageStore`] that can serve every published package
//! version. The validator's perpetual store gets all of these from
//! its [`AuthorityState`]; this adapter has only the typed
//! [`RpcStoreSchema`] CFs.
//!
//! Resolving layouts here requires:
//!
//! 1. A [`BackingPackageStore`] over `package_versions` + `objects`
//!    that returns the latest version of each requested package.
//! 2. An [`Executor`] / [`ProtocolConfig`] pair to drive
//!    `type_layout_resolver`.
//!
//! Both are non-trivial: building the executor requires choosing a
//! protocol version (the live one, from the latest epoch's system
//! state), and the executor's [`TypeLayoutStore`] uses the
//! [`BackingPackageStore`] internally. Wiring is left as a
//! follow-up; this stub returns `Ok(None)` so callers that ask for
//! a layout get a "no layout available" response rather than an
//! error.
//!
//! [`AuthorityState`]: https://docs.rs/sui-core/latest/sui_core/authority/struct.AuthorityState.html
//! [`Executor`]: sui_execution::Executor
//! [`ProtocolConfig`]: sui_types::sui_system_state::SuiSystemStateTrait
//! [`BackingPackageStore`]: sui_types::storage::BackingPackageStore
//! [`TypeLayoutStore`]: sui_types::layout_resolver::TypeLayoutStore

use move_core_types::annotated_value::MoveTypeLayout;
use move_core_types::language_storage::StructTag;
use sui_consistent_store::reader::Reader;
use sui_types::full_checkpoint_content::ObjectSet;
use sui_types::storage::error::Result as StorageResult;

use crate::reader::RpcStoreReader;

impl<R: Reader + Send + Sync> RpcStoreReader<R> {
    /// Resolve the [`MoveTypeLayout`] for a given [`StructTag`],
    /// optionally seeded with extra objects in `overlay`.
    ///
    /// Currently returns `Ok(None)` until the Move-resolver wiring
    /// lands. Callers receive a "no layout available" response,
    /// which the rpc-api surface treats as omit-the-layout rather
    /// than as an error.
    pub fn resolve_struct_layout(
        &self,
        _struct_tag: &StructTag,
        _overlay: &ObjectSet,
    ) -> StorageResult<Option<MoveTypeLayout>> {
        // TODO: wire a `BackingPackageStore` over `package_versions`
        // + `objects` and feed it into the executor's
        // `type_layout_resolver`. Needs a `ProtocolConfig` and an
        // `Executor` for the live protocol version; pulling those
        // from the latest epoch's `SuiSystemState` is the natural
        // path.
        Ok(None)
    }
}
