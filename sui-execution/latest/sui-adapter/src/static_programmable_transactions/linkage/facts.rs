// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{CompiledModule, file_format::Visibility};
use std::collections::BTreeMap;
use sui_types::base_types::ObjectID;

pub(crate) type LinkageFacts = BTreeMap<ObjectID, ObjectID>;
pub(crate) type ModuleInitFacts = BTreeMap<String, bool>;

/// Linkage-relevant facts for a single PTB command.
///
/// This is intentionally independent of both the loaded adapter transaction representation and raw
/// `ProgrammableTransaction` commands, so the unified-linkage constraint folding can be shared by
/// execution-time and signing-time analysis.
#[derive(Clone, Debug)]
pub(crate) enum LinkageCommandFacts {
    MoveCall {
        package: ObjectID,
        visibility: Visibility,
        type_defining_ids: Vec<ObjectID>,
    },
    Publish {
        has_init: bool,
        linkage: LinkageFacts,
    },
    Upgrade {
        current_package_id: ObjectID,
        current_module_inits: ModuleInitFacts,
        new_modules: Vec<CompiledModule>,
        linkage: LinkageFacts,
    },
    MakeMoveVec {
        type_defining_ids: Vec<ObjectID>,
    },
    Noop,
}
