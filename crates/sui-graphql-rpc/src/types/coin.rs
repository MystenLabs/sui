// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::data::{Db, QueryExecutor};
use crate::error::Error;

use super::big_int::BigInt;
use super::cursor::{Page, Target};
use super::move_object::MoveObject;
use super::object::{self, Object};
use super::sui_address::SuiAddress;
use async_graphql::*;

use async_graphql::connection::{Connection, CursorType, Edge};
use diesel::{ExpressionMethods, QueryDsl};
use sui_indexer::models_v2::objects::StoredObject;
use sui_indexer::schema_v2::objects;
use sui_indexer::types_v2::OwnerType;
use sui_types::coin::Coin as NativeCoin;
use sui_types::TypeTag;

#[derive(Clone)]
pub(crate) struct Coin {
    /// Representation of this Coin as a generic Move Object.
    pub super_: MoveObject,

    /// The deserialized representation of the Move Object's contents, as a `0x2::coin::Coin`.
    pub native: NativeCoin,
}

pub(crate) enum CoinDowncastError {
    NotACoin,
    Bcs(bcs::Error),
}

/// Some 0x2::coin::Coin Move object.
#[Object]
impl Coin {
    /// Balance of the coin object
    async fn balance(&self) -> Option<BigInt> {
        Some(BigInt::from(self.native.balance.value()))
    }

    /// Convert the coin object into a Move object
    async fn as_move_object(&self) -> &MoveObject {
        &self.super_
    }
}

impl Coin {
    /// Query the database for a `page` of coins. The page uses the bytes of an Object ID as the
    /// cursor, and can optionally be filtered by an owner.
    pub(crate) async fn paginate(
        db: &Db,
        page: Page<object::Cursor>,
        coin_type: TypeTag,
        owner: Option<SuiAddress>,
    ) -> Result<Connection<String, Coin>, Error> {
        let (prev, next, results) = db
            .execute(move |conn| {
                page.paginate_query::<StoredObject, _, _, _>(conn, move || {
                    use objects::dsl;
                    let mut query = dsl::objects.into_boxed();

                    query = query.filter(
                        dsl::coin_type.eq(coin_type.to_canonical_string(/* with_prefix */ true)),
                    );

                    if let Some(owner) = &owner {
                        // Leverage index on objects table
                        query = query.filter(dsl::owner_type.eq(OwnerType::Address as i16));
                        query = query.filter(dsl::owner_id.eq(owner.into_vec()));
                    }

                    query
                })
            })
            .await?;

        let mut conn = Connection::new(prev, next);

        for stored in results {
            let cursor = stored.cursor().encode_cursor();
            let object = Object::try_from(stored)?;

            let move_ = MoveObject::try_from(&object).map_err(|_| {
                Error::Internal(format!(
                    "Failed to deserialize as Move object: {}",
                    object.address
                ))
            })?;

            let coin = Coin::try_from(&move_).map_err(|_| {
                Error::Internal(format!("Faild to deserialize as Coin: {}", object.address))
            })?;

            conn.edges.push(Edge::new(cursor, coin));
        }

        Ok(conn)
    }
}

impl TryFrom<&MoveObject> for Coin {
    type Error = CoinDowncastError;

    fn try_from(move_object: &MoveObject) -> Result<Self, Self::Error> {
        if !move_object.native.is_coin() {
            return Err(CoinDowncastError::NotACoin);
        }

        Ok(Self {
            super_: move_object.clone(),
            native: bcs::from_bytes(move_object.native.contents())
                .map_err(CoinDowncastError::Bcs)?,
        })
    }
}
