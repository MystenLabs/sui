// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::checkpoint::Checkpoint;
use super::cursor::{self, Page, RawPaginated, Target};
use super::{big_int::BigInt, move_type::MoveType, sui_address::SuiAddress};
use crate::data::{Db, DbConnection, QueryExecutor, RawQuery};
use crate::error::Error;
use crate::{filter, query};
use async_graphql::connection::{Connection, CursorType, Edge};
use async_graphql::*;
use diesel::{
    sql_types::{BigInt as SqlBigInt, Nullable, Text},
    OptionalExtension, QueryableByName,
};
use std::str::FromStr;
use sui_types::{parse_sui_type_tag, TypeTag};

/// The total balance for a particular coin type.
#[derive(Clone, Debug, SimpleObject)]
pub(crate) struct Balance {
    /// Coin type for the balance, such as 0x2::sui::SUI
    pub(crate) coin_type: MoveType,
    /// How many coins of this type constitute the balance
    pub(crate) coin_object_count: Option<u64>,
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

pub(crate) type Cursor = cursor::JsonCursor<String>;

impl Balance {
    /// Query for the balance of coins owned by `address`, of coins with type `coin_type`. Note that
    /// `coin_type` is the type of `0x2::coin::Coin`'s type parameter, not the full type of the coin
    /// object.
    pub(crate) async fn query(
        db: &Db,
        address: SuiAddress,
        coin_type: TypeTag,
        checkpoint_sequence_number: Option<u64>,
    ) -> Result<Option<Balance>, Error> {
        let stored: Option<StoredBalance> = db
            .execute_repeatable(move |conn| {
                let (lhs, mut rhs) = Checkpoint::available_range(conn)?;

                if let Some(checkpoint_sequence_number) = checkpoint_sequence_number {
                    if checkpoint_sequence_number > rhs || checkpoint_sequence_number < lhs {
                        return Ok(None);
                    }
                    rhs = checkpoint_sequence_number;
                }

                conn.result(move || {
                    Balance::base_query(address, Some(coin_type.clone()), lhs as i64, rhs as i64)
                        .into_boxed()
                })
                .optional()
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
        checkpoint_sequence_number: Option<u64>,
    ) -> Result<Connection<String, Balance>, Error> {
        let response = db
            .execute_repeatable(move |conn| {
                let (lhs, mut rhs) = Checkpoint::available_range(conn)?;

                if let Some(checkpoint_sequence_number) = checkpoint_sequence_number {
                    if checkpoint_sequence_number > rhs || checkpoint_sequence_number < lhs {
                        return Ok(None);
                    }
                    rhs = checkpoint_sequence_number;
                }

                let result = page.paginate_raw_query::<StoredBalance>(conn, move || {
                    Balance::base_query(address, None, lhs as i64, rhs as i64)
                })?;

                Ok::<_, diesel::result::Error>(Some(result))
            })
            .await?;

        let mut conn = Connection::new(false, false);

        let Some((prev, next, results)) = response else {
            return Ok(conn);
        };

        conn.has_previous_page = prev;
        conn.has_next_page = next;

        for stored in results {
            let cursor = stored.cursor().encode_cursor();
            let balance = Balance::try_from(stored)?;
            conn.edges.push(Edge::new(cursor, balance));
        }

        Ok(conn)
    }

    fn base_query(address: SuiAddress, coin_type: Option<TypeTag>, lhs: i64, rhs: i64) -> RawQuery {
        let mut snapshot_objs = query!(r#"SELECT * FROM objects_snapshot"#);
        snapshot_objs = Balance::filter(snapshot_objs, address, coin_type.clone());

        let mut history_objs = query!(r#"SELECT * FROM objects_history"#);
        history_objs = Balance::filter(history_objs, address, coin_type.clone());
        history_objs = filter!(
            history_objs,
            format!(r#"checkpoint_sequence_number BETWEEN {} AND {}"#, lhs, rhs)
        );

        let final_ = consistent_object_read(snapshot_objs, history_objs, lhs, rhs);
        final_.group_by("candidates.coin_type")
    }

    fn filter(mut query: RawQuery, address: SuiAddress, coin_type: Option<TypeTag>) -> RawQuery {
        query = filter!(
            query,
            format!(
                "owner_id = '\\x{}'::bytea AND coin_type IS NOT NULL",
                hex::encode(address.into_vec())
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
}

impl RawPaginated<Cursor> for StoredBalance {
    fn filter_ge(cursor: &Cursor, query: RawQuery) -> RawQuery {
        filter!(query, "candidates.coin_type >= {}", (**cursor).clone())
    }

    fn filter_le(cursor: &Cursor, query: RawQuery) -> RawQuery {
        filter!(query, "candidates.coin_type <= {}", (**cursor).clone())
    }

    fn order(asc: bool, query: RawQuery) -> RawQuery {
        match asc {
            true => query.order_by("candidates.coin_type ASC"),
            false => query.order_by("candidates.coin_type DESC"),
        }
    }
}

impl Target<Cursor> for StoredBalance {
    fn cursor(&self) -> Cursor {
        Cursor::new(self.coin_type.clone())
    }
}

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

        let coin_object_count = count.map(|c| c as u64);

        let coin_type = MoveType::new(
            parse_sui_type_tag(&coin_type)
                .map_err(|e| Error::Internal(format!("Failed to parse coin type: {e}")))?,
        );

        Ok(Balance {
            coin_type,
            coin_object_count,
            total_balance,
        })
    }
}

pub(crate) fn build_candidates(snapshot_objs: RawQuery, history_objs: RawQuery) -> RawQuery {
    let candidates = query!(
        r#"SELECT DISTINCT ON (object_id) * FROM (
        ({})
        UNION
        ({})
    ) o"#,
        snapshot_objs,
        history_objs
    );

    candidates
        .order_by("object_id")
        .order_by("object_version DESC")
}

pub(crate) fn build_newer(lhs: i64, rhs: i64) -> RawQuery {
    let newer = query!(r#"SELECT object_id, object_version FROM objects_history"#);
    filter!(
        newer,
        format!(r#"checkpoint_sequence_number BETWEEN {} AND {}"#, lhs, rhs)
    )
}

pub(crate) fn build_join(candidates: RawQuery, newer: RawQuery) -> RawQuery {
    let final_ = query!(
        r#"
    SELECT CAST(SUM(candidates.coin_balance) AS TEXT) as balance, COUNT(*) as count, candidates.coin_type as coin_type
        FROM ({}) candidates
        LEFT JOIN ({}) newer
        ON (
            candidates.object_id = newer.object_id
            AND candidates.object_version < newer.object_version
        )"#,
        candidates,
        newer
    );
    filter!(final_, "newer.object_version IS NULL")
}

pub(crate) fn consistent_object_read(
    snapshot_objs: RawQuery,
    history_objs: RawQuery,
    lhs: i64,
    rhs: i64,
) -> RawQuery {
    let candidates = build_candidates(snapshot_objs, history_objs);
    let newer = build_newer(lhs, rhs);
    build_join(candidates, newer)
}
