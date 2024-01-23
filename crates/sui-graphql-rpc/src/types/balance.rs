// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::cursor::{self, BoxedPaginated, Page, Target};
use super::{big_int::BigInt, move_type::MoveType, sui_address::SuiAddress};
use crate::data::{Db, DbConnection, DieselBackend, QueryExecutor, RawQueryWrapper};
use crate::error::Error;
use crate::types::object::Object;
use async_graphql::connection::{Connection, CursorType, Edge};
use async_graphql::*;
use diesel::{sql_query, CombineDsl};
use diesel::{
    sql_types::{BigInt as SqlBigInt, Nullable, Text},
    ExpressionMethods, OptionalExtension, QueryDsl, QueryableByName,
};
use std::str::FromStr;
use sui_indexer::schema_v2::{checkpoints, objects_snapshot};
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
        checkpoint_sequence_number: Option<u64>,
        coin_type: TypeTag,
    ) -> Result<Option<Balance>, Error> {
        use checkpoints::dsl as checkpoints;
        use objects_snapshot::dsl as snapshot;

        let stored: Option<StoredBalance> = db
            .execute_repeatable(move |conn| {
                // If the checkpoint_sequence_number among cursor(s) and input is consistent, it
                // still needs to be within the graphql's availableRange
                let checkpoint_range: Vec<i64> = conn.results(move || {
                    let rhs = checkpoints::checkpoints
                        .select(checkpoints::sequence_number)
                        .order(checkpoints::sequence_number.desc());

                    let lhs = snapshot::objects_snapshot
                        .select(snapshot::checkpoint_sequence_number)
                        .order(snapshot::checkpoint_sequence_number.desc());

                    lhs.union(rhs)
                })?;

                let lhs: i64 = checkpoint_range.iter().min().copied().unwrap_or(0);
                let mut rhs: i64 = checkpoint_range.iter().max().copied().unwrap_or(0);

                if let Some(checkpoint_sequence_number) = checkpoint_sequence_number {
                    if checkpoint_sequence_number > rhs as u64
                        || checkpoint_sequence_number < lhs as u64
                    {
                        return Ok::<_, diesel::result::Error>(None);
                    }
                    rhs = checkpoint_sequence_number as i64;
                }

                conn.result_from_raw(move || {
                    let top_level_select = sql_query(r#"
                    SELECT CAST(SUM(candidates.coin_balance) AS TEXT) as balance, COUNT(*) as count, candidates.coin_type as coin_type FROM (
                        SELECT DISTINCT ON (object_id) * FROM (
                            SELECT * FROM objects_snapshot"#).into_boxed::<DieselBackend>();

                    let mut helper = RawQueryWrapper::new(top_level_select);

                    helper = Object::raw_coin_filter(helper, Some(coin_type.clone()), address);

                    helper = helper.sql(r#"
                    UNION
                    SELECT * FROM objects_history"#);

                    // history_query where clause -> WHERE (...)
                    helper.has_where_clause = false; // reset where clause
                    helper = Object::raw_coin_filter(helper, Some(coin_type.clone()), address);

                    let bind_1 = helper.get_bind_idx();
                    let bind_2 = helper.get_bind_idx();
                    let bind_3 = helper.get_bind_idx();
                    let bind_4 = helper.get_bind_idx();

                    helper = helper.sql(format!(r#"
                        ) o
                        WHERE checkpoint_sequence_number BETWEEN {} AND {} ORDER BY object_id, object_version DESC) candidates
                    LEFT JOIN (
                        SELECT object_id, object_version
                        FROM objects_history
                        WHERE checkpoint_sequence_number BETWEEN {} AND {}
                    ) newer
                    ON ( candidates.object_id = newer.object_id AND candidates.object_version < newer.object_version )
                    WHERE newer.object_version IS NULL
                    GROUP BY candidates.coin_type;
                    "#, bind_1, bind_2, bind_3, bind_4))
                    .bind::<diesel::sql_types::BigInt, _>(lhs)
                    .bind::<diesel::sql_types::BigInt, _>(rhs)
                    .bind::<diesel::sql_types::BigInt, _>(lhs)
                    .bind::<diesel::sql_types::BigInt, _>(rhs);

                     helper
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
        use checkpoints::dsl as checkpoints;
        use objects_snapshot::dsl as snapshot;
        let response= db
        .execute_repeatable(move |conn| {
            // If the checkpoint_sequence_number among cursor(s) and input is consistent, it
            // still needs to be within the graphql's availableRange
            let checkpoint_range: Vec<i64> = conn.results(move || {
                let rhs = checkpoints::checkpoints
                    .select(checkpoints::sequence_number)
                    .order(checkpoints::sequence_number.desc());

                let lhs = snapshot::objects_snapshot
                    .select(snapshot::checkpoint_sequence_number)
                    .order(snapshot::checkpoint_sequence_number.desc());

                lhs.union(rhs)
            })?;

            let lhs: i64 = checkpoint_range.iter().min().copied().unwrap_or(0);
            let mut rhs: i64 = checkpoint_range.iter().max().copied().unwrap_or(0);

            if let Some(checkpoint_sequence_number) = checkpoint_sequence_number {
                if checkpoint_sequence_number > rhs as u64
                    || checkpoint_sequence_number < lhs as u64
                {
                    return Ok::<_, diesel::result::Error>(None);
                }
                rhs = checkpoint_sequence_number as i64;
            }

            let result = page.paginate_raw_query::<StoredBalance, _>(conn,
                move |element| element.map(|balance_struct| balance_struct.cursor()),
                                move || {
                let top_level_select = sql_query(r#"
                SELECT CAST(SUM(candidates.coin_balance) AS TEXT) as balance, COUNT(*) as count, candidates.coin_type as coin_type FROM (
                    SELECT DISTINCT ON (object_id) * FROM (
                        SELECT * FROM objects_snapshot"#).into_boxed::<DieselBackend>();

                let mut helper = RawQueryWrapper::new(top_level_select);

                helper = Object::raw_coin_filter(helper, None, address);

                helper = helper.sql(r#"
                UNION
                SELECT * FROM objects_history"#);

                // history_query where clause -> WHERE (...)
                helper.has_where_clause = false; // reset where clause
                helper = Object::raw_coin_filter(helper, None, address);

                let bind_1 = helper.get_bind_idx();
                let bind_2 = helper.get_bind_idx();
                let bind_3 = helper.get_bind_idx();
                let bind_4 = helper.get_bind_idx();

                helper = helper.sql(format!(r#"
                    ) o
                    WHERE checkpoint_sequence_number BETWEEN {} AND {} ORDER BY object_id, object_version DESC) candidates
                LEFT JOIN (
                    SELECT object_id, object_version
                    FROM objects_history
                    WHERE checkpoint_sequence_number BETWEEN {} AND {}
                ) newer
                ON ( candidates.object_id = newer.object_id AND candidates.object_version < newer.object_version )
                WHERE newer.object_version IS NULL
                GROUP BY candidates.coin_type;
                "#, bind_1, bind_2, bind_3, bind_4))
                .bind::<diesel::sql_types::BigInt, _>(lhs)
                .bind::<diesel::sql_types::BigInt, _>(rhs)
                .bind::<diesel::sql_types::BigInt, _>(lhs)
                .bind::<diesel::sql_types::BigInt, _>(rhs);

                 helper
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
}

impl BoxedPaginated<Cursor> for StoredBalance {
    fn filter_ge(cursor: &Cursor, mut helper: RawQueryWrapper) -> RawQueryWrapper {
        let bind_idx = helper.get_bind_idx();
        let statement = helper.build_condition(format!("candidates.coin_type >= {}", bind_idx));
        helper
            .sql(statement)
            .bind::<diesel::sql_types::Text, _>((**cursor).clone())
    }

    fn filter_le(cursor: &Cursor, mut helper: RawQueryWrapper) -> RawQueryWrapper {
        let bind_idx = helper.get_bind_idx();
        let statement = helper.build_condition(format!("candidates.coin_type <= {}", bind_idx));
        helper
            .sql(statement)
            .bind::<diesel::sql_types::Text, _>((**cursor).clone())
    }

    fn order(asc: bool, helper: RawQueryWrapper) -> RawQueryWrapper {
        match asc {
            true => helper.sql(" ORDER BY candidates.coin_type ASC"),
            false => helper.sql(" ORDER BY  candidates.coin_type DESC"),
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
