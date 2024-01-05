// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    *,
};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer::models_v2::transactions::StoredTransaction;
use sui_types::{
    base_types::SuiAddress as NativeSuiAddress,
    transaction::{
        SenderSignedData as NativeSenderSignedData, TransactionDataAPI, TransactionExpiration,
    },
};

use crate::error::Error;

use super::{
    address::Address,
    base64::Base64,
    cursor::{Cursor, Page},
    digest::Digest,
    epoch::Epoch,
    event::Event,
    gas::GasInput,
    sui_address::SuiAddress,
    transaction_block_effects::TransactionBlockEffects,
    transaction_block_kind::TransactionBlockKind,
};

#[derive(Clone)]
pub(crate) struct TransactionBlock {
    /// Representation of transaction data in the Indexer's Store. The indexer stores the
    /// transaction data and its effects together, in one table.
    pub stored: StoredTransaction,

    /// Deserialized representation of `stored.raw_transaction`.
    pub native: NativeSenderSignedData,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum TransactionBlockKindInput {
    SystemTx = 0,
    ProgrammableTx = 1,
}

#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct TransactionBlockFilter {
    pub package: Option<SuiAddress>,
    pub module: Option<String>,
    pub function: Option<String>,

    pub kind: Option<TransactionBlockKindInput>,
    pub after_checkpoint: Option<u64>,
    pub at_checkpoint: Option<u64>,
    pub before_checkpoint: Option<u64>,

    pub sign_address: Option<SuiAddress>,
    pub sent_address: Option<SuiAddress>,
    pub recv_address: Option<SuiAddress>,
    pub paid_address: Option<SuiAddress>,

    pub input_object: Option<SuiAddress>,
    pub changed_object: Option<SuiAddress>,

    pub transaction_ids: Option<Vec<Digest>>,
}

pub(crate) type CTxEvent = Cursor<usize>;

#[Object]
impl TransactionBlock {
    /// A 32-byte hash that uniquely identifies the transaction block contents, encoded in Base58.
    /// This serves as a unique id for the block on chain.
    async fn digest(&self) -> String {
        Base58::encode(&self.stored.transaction_digest)
    }

    /// The address corresponding to the public key that signed this transaction. System
    /// transactions do not have senders.
    async fn sender(&self) -> Option<Address> {
        let sender = self.native.transaction_data().sender();
        (sender != NativeSuiAddress::ZERO).then(|| Address {
            address: SuiAddress::from(sender),
        })
    }

    /// The gas input field provides information on what objects were used as gas as well as the
    /// owner of the gas object(s) and information on the gas price and budget.
    ///
    /// If the owner of the gas object(s) is not the same as the sender, the transaction block is a
    /// sponsored transaction block.
    async fn gas_input(&self) -> Option<GasInput> {
        Some(GasInput::from(self.native.transaction_data().gas_data()))
    }

    /// The type of this transaction as well as the commands and/or parameters comprising the
    /// transaction of this kind.
    async fn kind(&self) -> Option<TransactionBlockKind> {
        Some(TransactionBlockKind::from(
            self.native.transaction_data().kind().clone(),
        ))
    }

    /// A list of all signatures, Base64-encoded, from senders, and potentially the gas owner if
    /// this is a sponsored transaction.
    async fn signatures(&self) -> Option<Vec<Base64>> {
        Some(
            self.native
                .tx_signatures()
                .iter()
                .map(|s| Base64::from(s.as_ref()))
                .collect(),
        )
    }

    /// The effects field captures the results to the chain of executing this transaction.
    async fn effects(&self) -> Result<Option<TransactionBlockEffects>> {
        Ok(Some(
            TransactionBlockEffects::try_from(self.stored.clone()).extend()?,
        ))
    }

    /// Events emitted by this transaction block.
    async fn events(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CTxEvent>,
        last: Option<u64>,
        before: Option<CTxEvent>,
    ) -> Result<Connection<String, Event>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let mut connection = Connection::new(false, false);
        let Some((prev, next, cs)) = page.paginate_indices(self.stored.events.len()) else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            let event = Event::try_from_stored_transaction(&self.stored, *c).extend()?;
            connection.edges.push(Edge::new(c.encode_cursor(), event));
        }

        Ok(connection)
    }

    /// This field is set by senders of a transaction block. It is an epoch reference that sets a
    /// deadline after which validators will no longer consider the transaction valid. By default,
    /// there is no deadline for when a transaction must execute.
    async fn expiration(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let TransactionExpiration::Epoch(id) = self.native.transaction_data().expiration() else {
            return Ok(None);
        };

        Epoch::query(ctx.data_unchecked(), Some(*id)).await.extend()
    }

    /// Serialized form of this transaction's `SenderSignedData`, BCS serialized and Base64 encoded.
    async fn bcs(&self) -> Option<Base64> {
        Some(Base64::from(&self.stored.raw_transaction))
    }
}

impl TryFrom<StoredTransaction> for TransactionBlock {
    type Error = Error;

    fn try_from(stored: StoredTransaction) -> Result<Self, Error> {
        let native = bcs::from_bytes(&stored.raw_transaction)
            .map_err(|e| Error::Internal(format!("Error deserializing transaction block: {e}")))?;

        Ok(TransactionBlock { stored, native })
    }
}
