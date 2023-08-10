// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

pub(crate) struct NameService;
pub(crate) struct NameServiceConnection;

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl NameService {
    async fn id(&self) -> ID {
        unimplemented!()
    }
}

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl NameServiceConnection {
    async fn id(&self) -> ID {
        unimplemented!()
    }
}
