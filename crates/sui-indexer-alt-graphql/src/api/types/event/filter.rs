// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::InputObject;

use crate::api::scalars::uint53::UInt53;

#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct EventFilter {
    /// Limit to events that occured strictly after the given checkpoint.
    pub after_checkpoint: Option<UInt53>,

    /// Limit to events in the given checkpoint.
    pub at_checkpoint: Option<UInt53>,

    /// Limit to event that occured strictly before the given checkpoint.
    pub before_checkpoint: Option<UInt53>,
    // TODO: (henry) Implement these filters.
    // pub sender: Option<SuiAddress>,
    // pub transaction_digest: Option<Digest>,
    // pub module: Option<ModuleFilter>,
    // pub type: Option<TypeFilter>,
}
