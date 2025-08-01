// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{connection::Connection, *};
use sui_types::{
    digests::ChainIdentifier as NativeChainIdentifier,
    transaction::EndOfEpochTransactionKind as NativeEndOfEpochTransactionKind,
};

use crate::{
    api::{
        scalars::cursor::JsonCursor, types::transaction_kind::change_epoch::ChangeEpochTransaction,
    },
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

type CTransaction = JsonCursor<usize>;

#[derive(Clone)]
pub struct EndOfEpochTransaction {
    pub native: Vec<NativeEndOfEpochTransactionKind>,
    pub scope: Scope,
}

#[derive(Union, Clone)]
pub enum EndOfEpochTransactionKind {
    ChangeEpoch(ChangeEpochTransaction),
    AuthenticatorStateCreate(AuthenticatorStateCreateTransaction),
    RandomnessStateCreate(RandomnessStateCreateTransaction),
    CoinDenyListStateCreate(CoinDenyListStateCreateTransaction),
    StoreExecutionTimeObservations(StoreExecutionTimeObservationsTransaction),
    BridgeStateCreate(BridgeStateCreateTransaction),
    AccumulatorRootCreate(AccumulatorRootCreateTransaction),
    // TODO: Add more complex transaction types incrementally
}

/// System transaction for creating the on-chain state used by zkLogin.
#[derive(SimpleObject, Clone)]
pub struct AuthenticatorStateCreateTransaction {
    /// A workaround to define an empty variant of a GraphQL union.
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

/// System transaction for creating the on-chain randomness state.
#[derive(SimpleObject, Clone)]
pub struct RandomnessStateCreateTransaction {
    /// A workaround to define an empty variant of a GraphQL union.
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

/// System transaction for creating the coin deny list state.
#[derive(SimpleObject, Clone)]
pub struct CoinDenyListStateCreateTransaction {
    /// A workaround to define an empty variant of a GraphQL union.
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

/// System transaction for storing execution time observations.
#[derive(SimpleObject, Clone)]
pub struct StoreExecutionTimeObservationsTransaction {
    /// A workaround to define an empty variant of a GraphQL union.
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

/// System transaction for creating bridge state.
#[derive(Clone)]
pub struct BridgeStateCreateTransaction {
    pub native: NativeChainIdentifier,
}

/// System transaction for creating bridge state for cross-chain operations.
#[Object]
impl BridgeStateCreateTransaction {
    /// The chain identifier for which this bridge state is being created.
    async fn chain_identifier(&self) -> Option<String> {
        Some(self.native.to_string())
    }
}

/// System transaction for creating the accumulator root.
#[derive(SimpleObject, Clone)]
pub struct AccumulatorRootCreateTransaction {
    /// A workaround to define an empty variant of a GraphQL union.
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

/// System transaction that supersedes `ChangeEpochTransaction` as the new way to run transactions at the end of an epoch. Behaves similarly to `ChangeEpochTransaction` but can accommodate other optional transactions to run at the end of the epoch.
#[Object]
impl EndOfEpochTransaction {
    /// The list of system transactions that did run at the end of the epoch.
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CTransaction>,
        last: Option<u64>,
        before: Option<CTransaction>,
    ) -> Result<Connection<String, EndOfEpochTransactionKind>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("EndOfEpochTransaction", "transactions");
        let page = Page::from_params(limits, first, after, last, before)?;

        let cursors = page.paginate_indices(self.native.len());
        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);

        for edge in cursors.edges {
            if let Some(tx_kind) = EndOfEpochTransactionKind::from(
                self.native[*edge.cursor].clone(),
                self.scope.clone(),
            ) {
                conn.edges.push(async_graphql::connection::Edge::new(
                    edge.cursor.to_string(),
                    tx_kind,
                ));
            }
        }

        Ok(conn)
    }
}

impl EndOfEpochTransactionKind {
    pub fn from(kind: NativeEndOfEpochTransactionKind, scope: Scope) -> Option<Self> {
        use EndOfEpochTransactionKind as K;
        use NativeEndOfEpochTransactionKind as N;

        match kind {
            N::ChangeEpoch(ce) => {
                Some(K::ChangeEpoch(ChangeEpochTransaction { native: ce, scope }))
            }
            N::AuthenticatorStateCreate => Some(K::AuthenticatorStateCreate(
                AuthenticatorStateCreateTransaction { dummy: None },
            )),
            N::RandomnessStateCreate => {
                Some(K::RandomnessStateCreate(RandomnessStateCreateTransaction {
                    dummy: None,
                }))
            }
            N::DenyListStateCreate => Some(K::CoinDenyListStateCreate(
                CoinDenyListStateCreateTransaction { dummy: None },
            )),
            N::StoreExecutionTimeObservations(_) => Some(K::StoreExecutionTimeObservations(
                StoreExecutionTimeObservationsTransaction { dummy: None },
            )),
            N::BridgeStateCreate(chain_id) => {
                Some(K::BridgeStateCreate(BridgeStateCreateTransaction {
                    native: chain_id,
                }))
            }
            N::AccumulatorRootCreate => {
                Some(K::AccumulatorRootCreate(AccumulatorRootCreateTransaction {
                    dummy: None,
                }))
            }
            // TODO: Handle more complex transaction types incrementally
            _ => None,
        }
    }
}
