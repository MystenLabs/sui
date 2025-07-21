// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    Context, Object,
};

use diesel::{ExpressionMethods, QueryDsl};
use fastcrypto::encoding::{Base58, Encoding};
use serde::{Deserialize, Serialize};
use sui_indexer_alt_reader::{
    kv_loader::{KvLoader, TransactionContents as NativeTransactionContents},
    pg_reader::PgReader,
};

use crate::{
    api::scalars::{base64::Base64, cursor::BcsCursor, digest::Digest},
    error::RpcError,
    pagination::Page,
    scope::Scope,
};

use super::{
    address::Address,
    epoch::Epoch,
    gas_input::GasInput,
    transaction_effects::{EffectsContents, TransactionEffects},
    transaction_filter::TransactionFilter,
    user_signature::UserSignature,
};
use sui_indexer_alt_schema::transactions::StoredTxDigest;
use sui_indexer_alt_schema::{schema::cp_sequence_numbers, schema::tx_digests};
use sui_types::{
    base_types::SuiAddress as NativeSuiAddress,
    digests::TransactionDigest,
    transaction::{TransactionDataAPI, TransactionExpiration},
};

#[derive(Clone)]
pub(crate) struct Transaction {
    pub(crate) digest: TransactionDigest,
    pub(crate) contents: TransactionContents,
}

#[derive(Clone)]
pub(crate) struct TransactionContents {
    pub(crate) scope: Scope,
    pub(crate) contents: Option<Arc<NativeTransactionContents>>,
}

/// The cursor returned for each `TransactionBlock` in a connection's page of results. The
/// `checkpoint_viewed_at` will set the consistent upper bound for subsequent queries made on this
/// cursor.
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone)]
pub(crate) struct TransactionCursor {
    #[serde(rename = "t")]
    pub tx_sequence_number: u64,
}

pub(crate) type CTransaction = BcsCursor<TransactionCursor>;

/// Description of a transaction, the unit of activity on Sui.
#[Object]
impl Transaction {
    /// A 32-byte hash that uniquely identifies the transaction contents, encoded in Base58.
    async fn digest(&self) -> String {
        Base58::encode(self.digest)
    }

    /// The results to the chain of executing this transaction.
    async fn effects(&self) -> Option<TransactionEffects> {
        Some(TransactionEffects::from(self.clone()))
    }

    #[graphql(flatten)]
    async fn contents(&self, ctx: &Context<'_>) -> Result<TransactionContents, RpcError> {
        self.contents.fetch(ctx, self.digest).await
    }
}

#[Object]
impl TransactionContents {
    /// This field is set by senders of a transaction block. It is an epoch reference that sets a deadline after which validators will no longer consider the transaction valid. By default, there is no deadline for when a transaction must execute.
    async fn expiration(&self) -> Result<Option<Epoch>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(None);
        };

        let transaction_data = content.data()?;
        match transaction_data.expiration() {
            TransactionExpiration::None => Ok(None),
            TransactionExpiration::Epoch(epoch_id) => {
                Ok(Some(Epoch::with_id(self.scope.clone(), *epoch_id)))
            }
        }
    }

    /// The gas input field provides information on what objects were used as gas as well as the owner of the gas object(s) and information on the gas price and budget.
    async fn gas_input(&self) -> Result<Option<GasInput>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(None);
        };

        let transaction_data = content.data()?;
        Ok(Some(GasInput::from_gas_data(
            self.scope.clone(),
            transaction_data.gas_data().clone(),
        )))
    }

    /// The address corresponding to the public key that signed this transaction. System transactions do not have senders.
    async fn sender(&self) -> Result<Option<Address>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(None);
        };

        let sender = content.data()?.sender();
        Ok((sender != NativeSuiAddress::ZERO)
            .then(|| Address::with_address(self.scope.clone(), sender)))
    }

    /// The Base64-encoded BCS serialization of this transaction, as a `TransactionData`.
    async fn transaction_bcs(&self) -> Result<Option<Base64>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(None);
        };

        Ok(Some(Base64(content.raw_transaction()?)))
    }

    /// User signatures for this transaction.
    async fn signatures(&self) -> Result<Vec<UserSignature>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(vec![]);
        };

        let signatures = content.signatures()?;
        Ok(signatures
            .into_iter()
            .map(UserSignature::from_generic_signature)
            .collect())
    }
}

impl Transaction {
    /// Construct a transaction that is represented by just its identifier (its transaction
    /// digest). This does not check whether the transaction exists, so should not be used to
    /// "fetch" a transaction based on a digest provided as user input.
    pub(crate) fn with_id(scope: Scope, digest: TransactionDigest) -> Self {
        Self {
            digest,
            contents: TransactionContents::empty(scope),
        }
    }

    /// Load the transaction from the store, and return it fully inflated (with contents already
    /// fetched). Returns `None` if the transaction does not exist (either never existed or was
    /// pruned from the store).
    pub(crate) async fn fetch(
        ctx: &Context<'_>,
        scope: Scope,
        digest: Digest,
    ) -> Result<Option<Self>, RpcError> {
        let contents = TransactionContents::empty(scope)
            .fetch(ctx, digest.into())
            .await?;

        let Some(tx) = &contents.contents else {
            return Ok(None);
        };

        Ok(Some(Self {
            digest: tx.digest()?,
            contents,
        }))
    }

    /// Cursor based pagination through transactions based on filters.
    /// Steps:
    /// 1. Determines the effective checkpoint range by:
    ///    - Converting filter parameters to checkpoint sequence numbers
    ///    - Computing the lower bound (cp_lo) as the maximum of after_checkpoint + 1 or at_checkpoint
    ///    - Computing the upper bound (cp_hi) as the minimum of`before_checkpoint - 1, at_checkpoint,
    ///      or the current checkpoint_viewed_at
    ///
    /// 2. Transaction Range Calculation: Maps checkpoint boundaries to transaction sequence numbers:
    ///    - Queries the cp_sequence_numbers table to find the first transaction (tx_lo) in the lower checkpoint
    ///    - Uses the `network_total_transactions` from the upper checkpoint to determine the exclusive upper bound
    ///
    /// 3. Database Query Construction: Builds a paginated query on the tx_digests table:
    ///    - Filters by transaction sequence number range
    ///    - Applies cursor-based pagination filters (after/before)
    ///    - Orders results by transaction sequence number (ascending or descending)
    ///    - Limits results to `page.limit() + 2` to determine pagination boundaries
    ///
    /// 4. Result Processing: Converts database results to GraphQL connection format:
    ///    - Maps stored transaction digests to Transaction objects
    ///    - Creates cursor objects for pagination
    ///    - Determines has_previous_page and has_next_page flags
    ///
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CTransaction>,
        filter: TransactionFilter,
    ) -> Result<Connection<String, Transaction>, RpcError> {
        use cp_sequence_numbers::dsl as cp;
        use tx_digests::dsl as dig;

        let mut conn = Connection::new(false, false);

        let cp_after = filter.after_checkpoint.map(u64::from);
        let cp_at = filter.at_checkpoint.map(u64::from);
        let cp_before = filter.before_checkpoint.map(u64::from);

        if filter.is_empty() || page.limit() == 0 {
            return Ok(Connection::new(false, false));
        }

        // TODO: Do we need to get the min_unpruned_checkpoint to handle if cp_after and cp_at are none, get min from Watermarks.pipeline_lo?
        let cp_lo = max_option([cp_after.map(|x| x.saturating_add(1)), cp_at]).unwrap();

        let cp_before_exclusive = match cp_before {
            // There are no results strictly before checkpoint 0.
            Some(0) => return Ok(Connection::new(false, false)),
            Some(x) => Some(x - 1),
            None => None,
        };

        // Inclusive upper bound in terms of checkpoint sequence number. If no upperbound is given,
        // use `checkpoint_viewed_at`.
        //
        // SAFETY: we can unwrap because of the `Some(checkpoint_viewed_at)
        let cp_hi = min_option([
            cp_before_exclusive,
            cp_at,
            Some(scope.checkpoint_viewed_at()),
        ])
        .unwrap();

        let pg_reader: &PgReader = ctx.data()?;
        let mut c = pg_reader
            .connect()
            .await
            .context("Failed to connect to database")?;

        let cp_tx_lo_query = cp::cp_sequence_numbers
            .select((cp::cp_sequence_number, cp::tx_lo))
            .filter(cp::cp_sequence_number.eq(cp_lo as i64))
            .limit(1)
            .order_by(cp::cp_sequence_number);

        let cp_tx_lo: Vec<(i64, i64)> = c
            .results(cp_tx_lo_query)
            .await
            .context("Failed to execute checkpoint bounds query")?;

        let Some(lo_record) = cp_tx_lo
            .iter()
            .find(|&(checkpoint, _)| *checkpoint == cp_lo as i64)
        else {
            return Ok(Connection::new(false, false));
        };
        let tx_lo = lo_record.1;

        let kv_loader: &KvLoader = ctx.data()?;
        let contents = kv_loader
            .load_one_checkpoint(cp_hi)
            .await
            .context("Failed to fetch checkpoint contents")?;

        // tx_hi_exclusive is the network_total_transactions of the highest checkpoint bound.
        let tx_hi_exclusive = if let Some((summary, _, _)) = contents.as_ref() {
            summary.network_total_transactions
        } else {
            return Ok(Connection::new(false, false));
        };

        let mut pagination = dig::tx_digests
            .filter(dig::tx_sequence_number.ge(tx_lo as i64))
            .filter(dig::tx_sequence_number.lt(tx_hi_exclusive as i64))
            .limit(page.limit() as i64 + 2)
            .into_boxed();

        if let Some(after) = page.after() {
            pagination =
                pagination.filter(dig::tx_sequence_number.ge(after.tx_sequence_number as i64));
        }

        if let Some(before) = page.before() {
            pagination =
                pagination.filter(dig::tx_sequence_number.lt(before.tx_sequence_number as i64));
        }

        pagination = if page.is_from_front() {
            pagination.order_by(dig::tx_sequence_number)
        } else {
            pagination.order_by(dig::tx_sequence_number.desc())
        };
        let mut c = pg_reader
            .connect()
            .await
            .context("Failed to connect to database")?;

        let results: Vec<StoredTxDigest> = c
            .results(pagination)
            .await
            .context("Failed to read from database")?;

        let (prev, next, results) = page.paginate_results(results, |t| {
            BcsCursor::new(TransactionCursor {
                tx_sequence_number: t.tx_sequence_number as u64,
            })
        });

        for (cursor, stored) in results {
            let transaction_digest = TransactionDigest::try_from(stored.tx_digest.clone())
                .context("Failed to deserialize transaction digest")?;
            let object = Self::with_id(scope.clone(), transaction_digest);
            conn.edges.push(Edge::new(cursor.encode_cursor(), object));
        }

        conn.has_previous_page = prev;
        conn.has_next_page = next;

        Ok(conn)
    }
}

impl TransactionContents {
    fn empty(scope: Scope) -> Self {
        Self {
            scope,
            contents: None,
        }
    }

    /// Attempt to fill the contents. If the contents are already filled, returns a clone,
    /// otherwise attempts to fetch from the store. The resulting value may still have an empty
    /// contents field, because it could not be found in the store.
    pub(crate) async fn fetch(
        &self,
        ctx: &Context<'_>,
        digest: TransactionDigest,
    ) -> Result<Self, RpcError> {
        if self.contents.is_some() {
            return Ok(self.clone());
        }

        let kv_loader: &KvLoader = ctx.data()?;
        let Some(transaction) = kv_loader
            .load_one_transaction(digest)
            .await
            .context("Failed to fetch transaction contents")?
        else {
            return Ok(self.clone());
        };

        // Discard the loaded result if we are viewing it at a checkpoint before it existed.
        if transaction.cp_sequence_number() > self.scope.checkpoint_viewed_at() {
            return Ok(self.clone());
        }

        Ok(Self {
            scope: self.scope.clone(),
            contents: Some(Arc::new(transaction)),
        })
    }
}

impl From<TransactionEffects> for Transaction {
    fn from(fx: TransactionEffects) -> Self {
        let EffectsContents { scope, contents } = fx.contents;

        Self {
            digest: fx.digest,
            contents: TransactionContents { scope, contents },
        }
    }
}

/// Determines the maximum value in an arbitrary number of Option<impl Ord>.
fn max_option<T: Ord>(xs: impl IntoIterator<Item = Option<T>>) -> Option<T> {
    xs.into_iter().flatten().max()
}

/// Determines the minimum value in an arbitrary number of Option<impl Ord>.
fn min_option<T: Ord>(xs: impl IntoIterator<Item = Option<T>>) -> Option<T> {
    xs.into_iter().flatten().min()
}
