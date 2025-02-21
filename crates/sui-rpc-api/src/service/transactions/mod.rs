// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::GetTransactionOptions;
use crate::types::TransactionResponse;
use crate::Result;
use crate::RpcService;
use sui_sdk_types::TransactionDigest;
use tap::Pipe;

mod execution;
mod resolve;

impl RpcService {
    pub fn get_transaction(
        &self,
        transaction_digest: TransactionDigest,
        options: &GetTransactionOptions,
    ) -> Result<TransactionResponse> {
        let crate::reader::TransactionRead {
            digest,
            transaction,
            signatures,
            effects,
            events,
            checkpoint,
            timestamp_ms,
        } = self.reader.get_transaction_read(transaction_digest)?;

        let transaction_bcs = options
            .include_transaction_bcs()
            .then(|| bcs::to_bytes(&transaction))
            .transpose()?;

        let effects_bcs = options
            .include_effects_bcs()
            .then(|| bcs::to_bytes(&effects))
            .transpose()?;

        let events_bcs = options
            .include_events_bcs()
            .then(|| events.as_ref().map(bcs::to_bytes))
            .flatten()
            .transpose()?;

        let signatures_bytes = options.include_signatures_bytes().then(|| {
            signatures
                .iter()
                .map(|signature| signature.to_bytes())
                .collect()
        });

        TransactionResponse {
            digest,
            transaction: options.include_transaction().then_some(transaction),
            transaction_bcs,
            signatures: options.include_signatures().then_some(signatures),
            signatures_bytes,
            effects: options.include_effects().then_some(effects),
            effects_bcs,
            events: options.include_events().then_some(events).flatten(),
            events_bcs,
            checkpoint,
            timestamp_ms,
        }
        .pipe(Ok)
    }
}
