// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{connection::Connection, *};
use sui_types::{
    digests::ChainIdentifier as NativeChainIdentifier,
    transaction::{
        AuthenticatorStateExpire as NativeAuthenticatorStateExpire,
        EndOfEpochTransactionKind as NativeEndOfEpochTransactionKind,
    },
};

use crate::{
    api::{
        scalars::{cursor::JsonCursor, uint53::UInt53},
        types::{epoch::Epoch, transaction_kind::change_epoch::ChangeEpochTransaction},
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
    AuthenticatorStateExpire(AuthenticatorStateExpireTransaction),
    RandomnessStateCreate(RandomnessStateCreateTransaction),
    CoinDenyListStateCreate(CoinDenyListStateCreateTransaction),
    StoreExecutionTimeObservations(StoreExecutionTimeObservationsTransaction),
    BridgeStateCreate(BridgeStateCreateTransaction),
    BridgeCommitteeInit(BridgeCommitteeInitTransaction),
    AccumulatorRootCreate(AccumulatorRootCreateTransaction),
    CoinRegistryCreate(CoinRegistryCreateTransaction),
    DisplayRegistryCreate(DisplayRegistryCreateTransaction),
    AddressAliasStateCreate(AddressAliasStateCreateTransaction),
    WriteAccumulatorStorageCost(WriteAccumulatorStorageCostTransaction),
    // TODO: Add more complex transaction types incrementally
}

/// System transaction for creating the on-chain state used by zkLogin.
#[derive(SimpleObject, Clone)]
pub struct AuthenticatorStateCreateTransaction {
    /// A workaround to define an empty variant of a GraphQL union.
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

#[derive(Clone)]
pub struct AuthenticatorStateExpireTransaction {
    pub native: NativeAuthenticatorStateExpire,
    pub scope: Scope,
}

/// System transaction that is executed at the end of an epoch to expire JSON Web Keys (JWKs) that are no longer valid, based on their associated epoch. This is part of the on-chain state management for zkLogin and authentication.
#[Object]
impl AuthenticatorStateExpireTransaction {
    /// Expire JWKs that have a lower epoch than this.
    async fn min_epoch(&self) -> Option<Epoch> {
        Some(Epoch::with_id(self.scope.clone(), self.native.min_epoch))
    }

    /// The initial version that the AuthenticatorStateUpdate was shared at.
    async fn authenticator_obj_initial_shared_version(&self) -> Option<UInt53> {
        Some(
            self.native
                .authenticator_obj_initial_shared_version
                .value()
                .into(),
        )
    }
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

/// System transaction for initializing bridge committee.
#[derive(SimpleObject, Clone)]
pub struct BridgeCommitteeInitTransaction {
    /// The initial shared version of the bridge object.
    bridge_object_version: Option<UInt53>,
}

/// System transaction for creating the accumulator root.
#[derive(SimpleObject, Clone)]
pub struct AccumulatorRootCreateTransaction {
    /// A workaround to define an empty variant of a GraphQL union.
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

/// System transaction for creating the coin registry.
#[derive(SimpleObject, Clone)]
pub struct CoinRegistryCreateTransaction {
    /// A workaround to define an empty variant of a GraphQL union.
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

/// System transaction for creating the display registry.
#[derive(SimpleObject, Clone)]
pub struct DisplayRegistryCreateTransaction {
    /// A workaround to define an empty variant of a GraphQL union.
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

/// System transaction for creating the alias state.
#[derive(SimpleObject, Clone)]
pub struct AddressAliasStateCreateTransaction {
    /// A workaround to define an empty variant of a GraphQL union.
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

/// System transaction for writing the pre-computed storage cost for accumulator objects.
#[derive(SimpleObject, Clone)]
pub struct WriteAccumulatorStorageCostTransaction {
    /// A workaround to define an empty variant of a GraphQL union.
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

/// System transaction that supersedes `ChangeEpochTransaction` as the new way to run transactions at the end of an epoch. Behaves similarly to `ChangeEpochTransaction` but can accommodate other optional transactions to run at the end of the epoch.
#[Object]
impl EndOfEpochTransaction {
    /// The list of system transactions that are allowed to run at the end of the epoch.
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CTransaction>,
        last: Option<u64>,
        before: Option<CTransaction>,
    ) -> Option<Result<Connection<String, EndOfEpochTransactionKind>, RpcError>> {
        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("EndOfEpochTransaction", "transactions");
                let page = Page::from_params(limits, first, after, last, before)?;

                page.paginate_indices(self.native.len(), |i| {
                    Ok(EndOfEpochTransactionKind::from(
                        self.native[i].clone(),
                        self.scope.clone(),
                    ))
                })
            }
            .await,
        )
    }
}

impl EndOfEpochTransactionKind {
    pub fn from(kind: NativeEndOfEpochTransactionKind, scope: Scope) -> Self {
        use EndOfEpochTransactionKind as K;
        use NativeEndOfEpochTransactionKind as N;

        match kind {
            N::ChangeEpoch(ce) => K::ChangeEpoch(ChangeEpochTransaction { native: ce, scope }),
            N::AuthenticatorStateCreate => {
                K::AuthenticatorStateCreate(AuthenticatorStateCreateTransaction { dummy: None })
            }
            N::AuthenticatorStateExpire(expire_data) => {
                K::AuthenticatorStateExpire(AuthenticatorStateExpireTransaction {
                    native: expire_data,
                    scope,
                })
            }
            N::RandomnessStateCreate => {
                K::RandomnessStateCreate(RandomnessStateCreateTransaction { dummy: None })
            }
            N::DenyListStateCreate => {
                K::CoinDenyListStateCreate(CoinDenyListStateCreateTransaction { dummy: None })
            }
            N::StoreExecutionTimeObservations(_) => {
                K::StoreExecutionTimeObservations(StoreExecutionTimeObservationsTransaction {
                    dummy: None,
                })
            }
            N::BridgeStateCreate(chain_id) => {
                K::BridgeStateCreate(BridgeStateCreateTransaction { native: chain_id })
            }
            N::BridgeCommitteeInit(bridge_version) => {
                K::BridgeCommitteeInit(BridgeCommitteeInitTransaction {
                    bridge_object_version: Some(bridge_version.value().into()),
                })
            }
            N::AccumulatorRootCreate => {
                K::AccumulatorRootCreate(AccumulatorRootCreateTransaction { dummy: None })
            }
            N::CoinRegistryCreate => {
                K::CoinRegistryCreate(CoinRegistryCreateTransaction { dummy: None })
            }
            N::DisplayRegistryCreate => {
                K::DisplayRegistryCreate(DisplayRegistryCreateTransaction { dummy: None })
            }
            N::AddressAliasStateCreate => {
                K::AddressAliasStateCreate(AddressAliasStateCreateTransaction { dummy: None })
            }
            N::WriteAccumulatorStorageCost(_) => {
                K::WriteAccumulatorStorageCost(WriteAccumulatorStorageCostTransaction {
                    dummy: None,
                })
            }
        }
    }
}
