// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::cursor::{self, Page, Target};
use super::{big_int::BigInt, move_type::MoveType, sui_address::SuiAddress};
use crate::data::{self, Db, DbConnection, QueryExecutor};
use crate::error::Error;
use async_graphql::connection::{Connection, CursorType, Edge};
use async_graphql::*;
use diesel::NullableExpressionMethods;
use diesel::{
    dsl::sql,
    sql_types::{BigInt as SqlBigInt, Nullable, Text},
    ExpressionMethods, OptionalExtension, QueryDsl,
};
use std::str::FromStr;
use sui_indexer::{schema_v2::objects, types_v2::OwnerType};
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
type StoredBalance = (
    /* balance */ Option<String>,
    /* count */ Option<i64>,
    /* type */ String,
);

pub(crate) type Cursor = cursor::JsonCursor<String>;
type Query<ST, GB> = data::Query<ST, objects::table, GB>;

impl Balance {
    /// Query for the balance of coins owned by `address`, of coins with type `coin_type`. Note that
    /// `coin_type` is the type of `0x2::coin::Coin`'s type parameter, not the full type of the coin
    /// object.
    pub(crate) async fn query(
        db: &Db,
        address: SuiAddress,
        coin_type: TypeTag,
    ) -> Result<Option<Balance>, Error> {
        use objects::dsl;

        let stored: Option<StoredBalance> = db
            .execute(move |conn| {
                conn.first(move || {
                    dsl::objects
                        .select((
                            sql::<Nullable<Text>>("CAST(SUM(coin_balance) AS TEXT)"),
                            sql::<Nullable<SqlBigInt>>("COUNT(*)"),
                            dsl::coin_type.assume_not_null(),
                        ))
                        .filter(dsl::owner_id.eq(address.into_vec()))
                        .filter(dsl::owner_type.eq(OwnerType::Address as i16))
                        .filter(
                            dsl::coin_type
                                .eq(coin_type.to_canonical_string(/* with_prefix */ true)),
                        )
                        .group_by(dsl::coin_type)
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
    ) -> Result<Connection<String, Balance>, Error> {
        let (prev, next, results) = db
            .execute(move |conn| {
                page.paginate_query::<StoredBalance, _, _, _>(conn, move || {
                    use objects::dsl;
                    dsl::objects
                        .select((
                            sql::<Nullable<Text>>("CAST(SUM(coin_balance) AS TEXT)"),
                            sql::<Nullable<SqlBigInt>>("COUNT(*)"),
                            dsl::coin_type.assume_not_null(),
                        ))
                        .filter(dsl::owner_id.eq(address.into_vec()))
                        .filter(dsl::owner_type.eq(OwnerType::Address as i16))
                        .filter(dsl::coin_type.is_not_null())
                        .group_by(dsl::coin_type)
                        .into_boxed()
                })
            })
            .await?;

        let mut conn = Connection::new(prev, next);

        for stored in results {
            let cursor = stored.cursor().encode_cursor();
            let balance = Balance::try_from(stored)?;
            conn.edges.push(Edge::new(cursor, balance));
        }

        Ok(conn)
    }
}

impl Target<Cursor> for StoredBalance {
    type Source = objects::table;

    fn filter_ge<ST, GB>(cursor: &Cursor, query: Query<ST, GB>) -> Query<ST, GB> {
        query.filter(objects::dsl::coin_type.ge((**cursor).clone()))
    }

    fn filter_le<ST, GB>(cursor: &Cursor, query: Query<ST, GB>) -> Query<ST, GB> {
        query.filter(objects::dsl::coin_type.le((**cursor).clone()))
    }

    fn order<ST, GB>(asc: bool, query: Query<ST, GB>) -> Query<ST, GB> {
        use objects::dsl;
        if asc {
            query.order_by(dsl::coin_type.asc())
        } else {
            query.order_by(dsl::coin_type.desc())
        }
    }

    fn cursor(&self) -> Cursor {
        Cursor::new(self.2.clone())
    }
}

impl TryFrom<StoredBalance> for Balance {
    type Error = Error;

    fn try_from((balance, count, coin_type): StoredBalance) -> Result<Self, Error> {
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
