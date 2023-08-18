// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::server::data_provider::DataProvider;
use async_graphql::Context;

pub(crate) trait DataProviderContextExt {
    fn data_provider(&self) -> &dyn DataProvider;
}

impl DataProviderContextExt for Context<'_> {
    fn data_provider(&self) -> &dyn DataProvider {
        &**self.data_unchecked::<Box<dyn DataProvider>>()
    }
}
