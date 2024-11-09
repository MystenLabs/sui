// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::available_range::AvailableRange;
use super::cursor::{self, Page, RawPaginated, ScanLimited, Target};
use super::uint53::UInt53;
use super::{big_int::BigInt, move_type::MoveType, sui_address::SuiAddress};
use crate::consistency::Checkpointed;
use crate::data::{Db, DbConnection, QueryExecutor};
use crate::error::Error;
use crate::raw_query::RawQuery;
use crate::{filter, query};
use async_graphql::connection::{Connection, CursorType, Edge};
use async_graphql::*;
use diesel::{
    sql_types::{BigInt as SqlBigInt, Nullable, Text},
    OptionalExtension, QueryableByName,
};
use diesel_async::scoped_futures::ScopedFutureExt;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use sui_indexer::types::OwnerType;
use sui_types::TypeTag;

/// The total balance for a particular coin type.
#[derive(Clone, Debug, SimpleObject)]
pub(crate) struct Balance {
    /// Coin type for the balance, such as 0x2::sui::SUI
    pub(crate) coin_type: MoveType,
    /// How many coins of this type constitute the balance
    pub(crate) coin_object_count: Option<UInt53>,
    /// Total balance across all coin objects of the coin type
    pub(crate) total_balance: Option<BigInt>,
}

/// Representation of a row of balance information from the DB. We read the balance as a `String` to
/// deal with the large (bigger than 2^63 - 1) balances.
#[derive(QueryableByName)]
pub struct StoredBalance {
    #[diesel(sql_type = Nullable<Text>)]
    pub balance: Option<String>,
    #[diesel(sql_type = Nullable<SqlBigInt>)]
    pub count: Option<i64>,
    #[diesel(sql_type = Text)]
    pub coin_type: String,
}

pub(crate) type Cursor = cursor::JsonCursor<BalanceCursor>;

/// The inner struct for the `Balance`'s cursor. The `coin_type` is used as the cursor, while the
/// `checkpoint_viewed_at` sets the consistent upper bound for the cursor.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub(crate) struct BalanceCursor {
    #[serde(rename = "t")]
    coin_type: String,
    /// The checkpoint sequence number this was viewed at.
    #[serde(rename = "c")]
    checkpoint_viewed_at: u64,
}

impl Balance {
    /// Query for the balance of coins owned by `address`, of coins with type `coin_type`. Note that
    /// `coin_type` is the type of `0x2::coin::Coin`'s type parameter, not the full type of the coin
    /// object.
    pub(crate) async fn query(
        db: &Db,
        address: SuiAddress,
        coin_type: TypeTag,
        checkpoint_viewed_at: u64,
    ) -> Result<Option<Balance>, Error> {
        let stored: Option<StoredBalance> = db
            .execute_repeatable(move |conn| {
                async move {
                    let Some(range) = AvailableRange::result(conn, checkpoint_viewed_at).await?
                    else {
                        return Ok::<_, diesel::result::Error>(None);
                    };

                    conn.result(move || {
                        balance_query(address, Some(coin_type.clone()), range).into_boxed()
                    })
                    .await
                    .optional()
                }
                .scope_boxed()
            })
            .await?;

        stored.map(Balance::try_from).transpose()
    }

    /// Query the database for a `page` of coin balances. Each balance represents the total balance
    /// for a particular coin type, owned by `address`.
    pub(crate) async fn paginate(
        db: &Db,
        page: Page<Cursor>,
        address: SuiAddress,
        checkpoint_viewed_at: u64,
    ) -> Result<Connection<String, Balance>, Error> {
        // If cursors are provided, defer to the `checkpoint_viewed_at` in the cursor if they are
        // consistent. Otherwise, use the value from the parameter, or set to None. This is so that
        // paginated queries are consistent with the previous query that created the cursor.
        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at = cursor_viewed_at.unwrap_or(checkpoint_viewed_at);

        let Some((prev, next, results)) = db
            .execute_repeatable(move |conn| {
                async move {
                    let Some(range) = AvailableRange::result(conn, checkpoint_viewed_at).await?
                    else {
                        return Ok::<_, diesel::result::Error>(None);
                    };

                    let result = page
                        .paginate_raw_query::<StoredBalance>(
                            conn,
                            checkpoint_viewed_at,
                            balance_query(address, None, range),
                        )
                        .await?;

                    Ok(Some(result))
                }
                .scope_boxed()
            })
            .await?
        else {
            return Err(Error::Client(
                "Requested data is outside the available range".to_string(),
            ));
        };

        let mut conn = Connection::new(prev, next);
        for stored in results {
            let cursor = stored.cursor(checkpoint_viewed_at).encode_cursor();
            let balance = Balance::try_from(stored)?;
            conn.edges.push(Edge::new(cursor, balance));
        }

        Ok(conn)
    }
}

impl RawPaginated<Cursor> for StoredBalance {
    fn filter_ge(cursor: &Cursor, query: RawQuery) -> RawQuery {
        filter!(query, "coin_type >= {}", cursor.coin_type.clone())
    }

    fn filter_le(cursor: &Cursor, query: RawQuery) -> RawQuery {
        filter!(query, "coin_type <= {}", cursor.coin_type.clone())
    }

    fn order(asc: bool, query: RawQuery) -> RawQuery {
        if asc {
            return query.order_by("coin_type ASC");
        }
        query.order_by("coin_type DESC")
    }
}

impl Target<Cursor> for StoredBalance {
    fn cursor(&self, checkpoint_viewed_at: u64) -> Cursor {
        Cursor::new(BalanceCursor {
            coin_type: self.coin_type.clone(),
            checkpoint_viewed_at,
        })
    }
}

impl Checkpointed for Cursor {
    fn checkpoint_viewed_at(&self) -> u64 {
        self.checkpoint_viewed_at
    }
}

impl ScanLimited for Cursor {}

impl TryFrom<StoredBalance> for Balance {
    type Error = Error;

    fn try_from(s: StoredBalance) -> Result<Self, Error> {
        let StoredBalance {
            balance,
            count,
            coin_type,
        } = s;
        let total_balance = balance
            .map(|b| BigInt::from_str(&b))
            .transpose()
            .map_err(|_| Error::Internal("Failed to read balance.".to_string()))?;

        let coin_object_count = count.map(|c| UInt53::from(c as u64));

        let coin_type = TypeTag::from_str(&coin_type)
            .map_err(|e| Error::Internal(format!("Failed to parse coin type: {e}")))?
            .into();

        Ok(Balance {
            coin_type,
            coin_object_count,
            total_balance,
        })
    }
}

/// Query the database for a `page` of coin balances. Each balance represents the total balance for
/// a particular coin type, owned by `address`. This function is meant to be called within a thunk
/// and returns a RawQuery that can be converted into a BoxedSqlQuery with `.into_boxed()`.
fn balance_query(
    address: SuiAddress,
    coin_type: Option<TypeTag>,
    range: AvailableRange,
) -> RawQuery {
    // Construct the filtered inner query - apply the same filtering criteria to both
    // objects_snapshot and objects_history tables.
    let mut snapshot_objs = query!("SELECT * FROM objects_snapshot");
    snapshot_objs = filter(snapshot_objs, address, coin_type.clone());

    // Additionally filter objects_history table for results between the available range, or
    // checkpoint_viewed_at, if provided.
    let mut history_objs = query!("SELECT * FROM objects_history");
    history_objs = filter(history_objs, address, coin_type.clone());
    history_objs = filter!(
        history_objs,
        format!(
            r#"checkpoint_sequence_number BETWEEN {} AND {}"#,
            range.first, range.last
        )
    );

    // Combine the two queries, and select the most recent version of each object.
    let candidates = query!(
        r#"SELECT DISTINCT ON (object_id) * FROM (({}) UNION ALL ({})) o"#,
        snapshot_objs,
        history_objs
    )
    .order_by("object_id")
    .order_by("object_version DESC");

    // Objects that fulfill the filtering criteria may not be the most recent version available.
    // Left join the candidates table on newer to filter out any objects that have a newer
    // version.
    let mut newer = query!("SELECT object_id, object_version FROM objects_history");
    newer = filter!(
        newer,
        format!(
            r#"checkpoint_sequence_number BETWEEN {} AND {}"#,
            range.first, range.last
        )
    );
    let final_ = query!(
        r#"SELECT
            CAST(SUM(coin_balance) AS TEXT) as balance,
            COUNT(*) as count,
            coin_type
        FROM ({}) candidates
        LEFT JOIN ({}) newer
        ON (
            candidates.object_id = newer.object_id
            AND candidates.object_version < newer.object_version
        )"#,
        candidates,
        newer
    );

    // Additionally for balance's query, group coins by coin_type.
    filter!(final_, "newer.object_version IS NULL").group_by("coin_type")
}

/// Applies the filtering criteria for balances to the input `RawQuery` and returns a new
/// `RawQuery`.
fn filter(mut query: RawQuery, owner: SuiAddress, coin_type: Option<TypeTag>) -> RawQuery {
    query = filter!(query, "coin_type IS NOT NULL AND object_status = 0");

    query = filter!(
        query,
        format!(
            "owner_id = '\\x{}'::bytea AND owner_type = {}",
            hex::encode(owner.into_vec()),
            OwnerType::Address as i16
        )
    );

    if let Some(coin_type) = coin_type {
        query = filter!(
            query,
            "coin_type = {}",
            coin_type.to_canonical_display(/* with_prefix */ true)
        );
    };

    query
}
