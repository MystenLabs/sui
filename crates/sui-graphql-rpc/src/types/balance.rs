// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{big_int::BigInt, move_type::MoveType, sui_address::SuiAddress};
use crate::data::{Db, DbConnection, QueryExecutor};
use crate::error::Error;
use async_graphql::*;
use diesel::{
    dsl::sql,
    sql_types::{BigInt as SqlBigInt, Nullable},
    ExpressionMethods, OptionalExtension, QueryDsl,
};
use sui_indexer::{schema_v2::objects, types_v2::OwnerType};
use sui_types::{parse_sui_type_tag, TypeTag};

/// The total balance for a particular coin type.
#[derive(Clone, Debug, SimpleObject)]
pub(crate) struct Balance {
    /// Coin type for the balance, such as 0x2::sui::SUI
    pub(crate) coin_type: Option<MoveType>,
    /// How many coins of this type constitute the balance
    pub(crate) coin_object_count: Option<u64>,
    /// Total balance across all coin objects of the coin type
    pub(crate) total_balance: Option<BigInt>,
}

type StoredBalance = (
    /* balance */ Option<i64>,
    /* count */ Option<i64>,
    /* type */ Option<String>,
);

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
                            sql::<Nullable<SqlBigInt>>("CAST(SUM(coin_balance) AS BIGINT)"),
                            sql::<Nullable<SqlBigInt>>("COUNT(*)"),
                            dsl::coin_type,
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
}

impl TryFrom<StoredBalance> for Balance {
    type Error = Error;

    fn try_from((balance, count, coin_type): StoredBalance) -> Result<Self, Error> {
        let total_balance = balance.map(BigInt::from);
        let coin_object_count = count.map(|c| c as u64);
        let coin_type = coin_type
            .as_deref()
            .map(parse_sui_type_tag)
            .transpose()
            .map_err(|e| Error::Internal(format!("Failed to parse coin type: {e}")))?
            .map(MoveType::new);

        Ok(Balance {
            coin_type,
            coin_object_count,
            total_balance,
        })
    }
}
