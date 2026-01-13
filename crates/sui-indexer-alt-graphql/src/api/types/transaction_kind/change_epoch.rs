// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::Object;
use async_graphql::connection::Connection;
use move_binary_format::CompiledModule;
use move_binary_format::errors::PartialVMResult;
use sui_types::digests::TransactionDigest;
use sui_types::object::Object as NativeObject;
use sui_types::transaction::ChangeEpoch as NativeChangeEpoch;

use crate::api::scalars::cursor::JsonCursor;
use crate::api::scalars::date_time::DateTime;
use crate::api::scalars::uint53::UInt53;
use crate::api::types::epoch::Epoch;
use crate::api::types::move_package::MovePackage;
use crate::api::types::protocol_configs::ProtocolConfigs;
use crate::error::RpcError;
use crate::pagination::Page;
use crate::pagination::PaginationConfig;
use crate::scope::Scope;

#[derive(Clone)]
pub(crate) struct ChangeEpochTransaction {
    pub(crate) scope: Scope,
    pub(crate) native: NativeChangeEpoch,
}

pub(crate) type CSystemPackage = JsonCursor<usize>;

/// A system transaction that updates epoch information on-chain (increments the current epoch). Executed by the system once per epoch, without using gas. Epoch change transactions cannot be submitted by users, because validators will refuse to sign them.
///
/// This transaction kind is deprecated in favour of `EndOfEpochTransaction`.
#[Object]
impl ChangeEpochTransaction {
    /// The next (to become) epoch.
    async fn epoch(&self) -> Option<Epoch> {
        Some(Epoch::with_id(self.scope.clone(), self.native.epoch))
    }

    /// The epoch's corresponding protocol configuration.
    async fn protocol_configs(&self) -> Option<ProtocolConfigs> {
        Some(ProtocolConfigs::with_protocol_version(
            self.native.protocol_version.as_u64(),
        ))
    }

    /// The total amount of gas charged for storage during the epoch.
    async fn storage_charge(&self) -> Option<UInt53> {
        Some(self.native.storage_charge.into())
    }

    /// The total amount of gas charged for computation during the epoch.
    async fn computation_charge(&self) -> Option<UInt53> {
        Some(self.native.computation_charge.into())
    }

    /// The amount of storage rebate refunded to the transaction senders.
    async fn storage_rebate(&self) -> Option<UInt53> {
        Some(self.native.storage_rebate.into())
    }

    /// The non-refundable storage fee.
    async fn non_refundable_storage_fee(&self) -> Option<UInt53> {
        Some(self.native.non_refundable_storage_fee.into())
    }

    /// Unix timestamp when epoch started.
    async fn epoch_start_timestamp(&self) -> Option<Result<DateTime, RpcError>> {
        Some(DateTime::from_ms(
            self.native.epoch_start_timestamp_ms as i64,
        ))
    }

    /// System packages that will be written by validators before the new epoch starts, to upgrade them on-chain. These objects do not have a "previous transaction" because they are not written on-chain yet. Consult `effects.objectChanges` for this transaction to see the actual objects written.
    async fn system_packages(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CSystemPackage>,
        last: Option<u64>,
        before: Option<CSystemPackage>,
    ) -> Option<Result<Connection<String, MovePackage>, RpcError>> {
        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("ChangeEpochTransaction", "systemPackages");
                let page = Page::from_params(limits, first, after, last, before)?;

                page.paginate_indices(self.native.system_packages.len(), |i| {
                    let (version, modules, deps) = &self.native.system_packages[i];

                    // Deserialize the compiled modules
                    let compiled_modules = modules
                        .iter()
                        .map(|bytes| CompiledModule::deserialize_with_defaults(bytes))
                        .collect::<PartialVMResult<Vec<_>>>()
                        .context("Failed to deserialize system modules")?;

                    // Create a native system package object
                    let native_object = NativeObject::new_system_package(
                        &compiled_modules,
                        *version,
                        deps.clone(),
                        TransactionDigest::ZERO,
                    );

                    // Create MovePackage directly from native object for efficiency
                    let package =
                        MovePackage::from_native_object(self.scope.clone(), native_object)
                            .context("Failed to create MovePackage from system package object")?;

                    Ok(package)
                })
            }
            .await,
        )
    }
}
