// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::AuthorityAPI;

#[cfg(test)]
pub(crate) mod tests;

use super::ActiveAuthority;

pub async fn checkpoint_process<A>(_active_authority: &ActiveAuthority<A>)
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
}
