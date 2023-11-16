// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    db_backend::{BalanceQuery, GenericQueryBuilder},
    db_data_provider::DbValidationError,
};
use crate::{
    error::Error,
    types::{digest::Digest, object::ObjectFilter, transaction_block::TransactionBlockFilter},
};
use diesel::{
    pg::Pg,
    query_builder::{BoxedSelectStatement, FromClause},
    BoolExpressionMethods, ExpressionMethods, QueryDsl,
};
use std::str::FromStr;
use sui_indexer::{
    schema_v2::{
        checkpoints, epochs, objects, transactions, tx_calls, tx_changed_objects, tx_input_objects,
        tx_recipients, tx_senders,
    },
    types_v2::OwnerType,
};

type PgBalanceQuery<'a> = BoxedSelectStatement<
    'a,
    (
        diesel::sql_types::Nullable<diesel::sql_types::BigInt>,
        diesel::sql_types::Nullable<diesel::sql_types::BigInt>,
        diesel::sql_types::Nullable<diesel::sql_types::Text>,
    ),
    FromClause<objects::table>,
    Pg,
    objects::dsl::coin_type,
>;

pub(crate) struct PgQueryBuilder;
impl PgQueryBuilder {
    fn get_tx_by_digest<'a>(digest: Vec<u8>) -> transactions::BoxedQuery<'a, Pg> {
        transactions::dsl::transactions
            .filter(transactions::dsl::transaction_digest.eq(digest))
            .into_boxed()
    }

    fn get_obj<'a>(address: Vec<u8>, version: Option<i64>) -> objects::BoxedQuery<'a, Pg> {
        let mut query = objects::dsl::objects.into_boxed();
        query = query.filter(objects::dsl::object_id.eq(address));

        if let Some(version) = version {
            query = query.filter(objects::dsl::object_version.eq(version));
        }
        query
    }

    fn get_epoch<'a>(epoch_id: i64) -> epochs::BoxedQuery<'a, Pg> {
        epochs::dsl::epochs
            .filter(epochs::dsl::epoch.eq(epoch_id))
            .into_boxed()
    }

    fn get_latest_epoch<'a>() -> epochs::BoxedQuery<'a, Pg> {
        epochs::dsl::epochs
            .order_by(epochs::dsl::epoch.desc())
            .limit(1)
            .into_boxed()
    }

    fn get_checkpoint_by_digest<'a>(digest: Vec<u8>) -> checkpoints::BoxedQuery<'a, Pg> {
        checkpoints::dsl::checkpoints
            .filter(checkpoints::dsl::checkpoint_digest.eq(digest))
            .into_boxed()
    }

    fn get_checkpoint_by_sequence_number<'a>(
        sequence_number: i64,
    ) -> checkpoints::BoxedQuery<'a, Pg> {
        checkpoints::dsl::checkpoints
            .filter(checkpoints::dsl::sequence_number.eq(sequence_number))
            .into_boxed()
    }

    fn get_latest_checkpoint<'a>() -> checkpoints::BoxedQuery<'a, Pg> {
        checkpoints::dsl::checkpoints
            .order_by(checkpoints::dsl::sequence_number.desc())
            .limit(1)
            .into_boxed()
    }

    fn multi_get_txs<'a>(
        cursor: Option<i64>,
        descending_order: bool,
        limit: i64,
        filter: Option<TransactionBlockFilter>,
        after_tx_seq_num: Option<i64>,
        before_tx_seq_num: Option<i64>,
    ) -> Result<transactions::BoxedQuery<'a, Pg>, Error> {
        let mut query = transactions::dsl::transactions.into_boxed();

        if let Some(cursor_val) = cursor {
            if descending_order {
                let filter_value =
                    before_tx_seq_num.map_or(cursor_val, |b| std::cmp::min(b, cursor_val));
                query = query.filter(transactions::dsl::tx_sequence_number.lt(filter_value));
            } else {
                let filter_value =
                    after_tx_seq_num.map_or(cursor_val, |a| std::cmp::max(a, cursor_val));
                query = query.filter(transactions::dsl::tx_sequence_number.gt(filter_value));
            }
        } else {
            if let Some(av) = after_tx_seq_num {
                query = query.filter(transactions::dsl::tx_sequence_number.gt(av));
            }
            if let Some(bv) = before_tx_seq_num {
                query = query.filter(transactions::dsl::tx_sequence_number.lt(bv));
            }
        }

        if descending_order {
            query = query.order(transactions::dsl::tx_sequence_number.desc());
        } else {
            query = query.order(transactions::dsl::tx_sequence_number.asc());
        }

        query = query.limit(limit + 1);

        if let Some(filter) = filter {
            // Filters for transaction table
            // at_checkpoint mutually exclusive with before_ and after_checkpoint
            if let Some(checkpoint) = filter.at_checkpoint {
                query = query
                    .filter(transactions::dsl::checkpoint_sequence_number.eq(checkpoint as i64));
            }
            if let Some(transaction_ids) = filter.transaction_ids {
                let digests = transaction_ids
                    .into_iter()
                    .map(|id| Ok::<Vec<u8>, Error>(Digest::from_str(&id)?.into_vec()))
                    .collect::<Result<Vec<_>, _>>()?;
                query = query.filter(transactions::dsl::transaction_digest.eq_any(digests));
            }

            // Queries on foreign tables
            match (filter.package, filter.module, filter.function) {
                (Some(p), None, None) => {
                    let subquery = tx_calls::dsl::tx_calls
                        .filter(tx_calls::dsl::package.eq(p.into_vec()))
                        .select(tx_calls::dsl::tx_sequence_number);

                    query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
                }
                (Some(p), Some(m), None) => {
                    let subquery = tx_calls::dsl::tx_calls
                        .filter(tx_calls::dsl::package.eq(p.into_vec()))
                        .filter(tx_calls::dsl::module.eq(m))
                        .select(tx_calls::dsl::tx_sequence_number);

                    query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
                }
                (Some(p), Some(m), Some(f)) => {
                    let subquery = tx_calls::dsl::tx_calls
                        .filter(tx_calls::dsl::package.eq(p.into_vec()))
                        .filter(tx_calls::dsl::module.eq(m))
                        .filter(tx_calls::dsl::func.eq(f))
                        .select(tx_calls::dsl::tx_sequence_number);

                    query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
                }
                _ => {}
            }

            if let Some(signer) = filter.sign_address {
                if let Some(sender) = filter.sent_address {
                    let subquery = tx_senders::dsl::tx_senders
                        .filter(
                            tx_senders::dsl::sender
                                .eq(signer.into_vec())
                                .or(tx_senders::dsl::sender.eq(sender.into_vec())),
                        )
                        .select(tx_senders::dsl::tx_sequence_number);

                    query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
                } else {
                    let subquery = tx_senders::dsl::tx_senders
                        .filter(tx_senders::dsl::sender.eq(signer.into_vec()))
                        .select(tx_senders::dsl::tx_sequence_number);

                    query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
                }
            } else if let Some(sender) = filter.sent_address {
                let subquery = tx_senders::dsl::tx_senders
                    .filter(tx_senders::dsl::sender.eq(sender.into_vec()))
                    .select(tx_senders::dsl::tx_sequence_number);

                query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
            }
            if let Some(recipient) = filter.recv_address {
                let subquery = tx_recipients::dsl::tx_recipients
                    .filter(tx_recipients::dsl::recipient.eq(recipient.into_vec()))
                    .select(tx_recipients::dsl::tx_sequence_number);

                query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
            }
            if filter.paid_address.is_some() {
                return Err(Error::Internal(
                    "Paid address filter not supported".to_string(),
                ));
            }

            if let Some(input_object) = filter.input_object {
                let subquery = tx_input_objects::dsl::tx_input_objects
                    .filter(tx_input_objects::dsl::object_id.eq(input_object.into_vec()))
                    .select(tx_input_objects::dsl::tx_sequence_number);

                query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
            }
            if let Some(changed_object) = filter.changed_object {
                let subquery = tx_changed_objects::dsl::tx_changed_objects
                    .filter(tx_changed_objects::dsl::object_id.eq(changed_object.into_vec()))
                    .select(tx_changed_objects::dsl::tx_sequence_number);

                query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
            }
        };

        Ok(query)
    }

    fn multi_get_coins<'a>(
        cursor: Option<Vec<u8>>,
        descending_order: bool,
        limit: i64,
        address: Vec<u8>,
        coin_type: String,
    ) -> objects::BoxedQuery<'a, Pg> {
        let mut query = objects::dsl::objects.into_boxed();
        if let Some(cursor) = cursor {
            if descending_order {
                query = query.filter(objects::dsl::object_id.lt(cursor));
            } else {
                query = query.filter(objects::dsl::object_id.gt(cursor));
            }
        }
        if descending_order {
            query = query.order(objects::dsl::object_id.desc());
        } else {
            query = query.order(objects::dsl::object_id.asc());
        }
        query = query.limit(limit + 1);

        query = query
            .filter(objects::dsl::owner_id.eq(address))
            .filter(objects::dsl::owner_type.eq(OwnerType::Address as i16)) // Leverage index on objects table
            .filter(objects::dsl::coin_type.eq(coin_type));

        query
    }

    fn multi_get_objs<'a>(
        cursor: Option<Vec<u8>>,
        descending_order: bool,
        limit: i64,
        filter: Option<ObjectFilter>,
        owner_type: Option<OwnerType>,
    ) -> Result<objects::BoxedQuery<'a, Pg>, Error> {
        let mut query = objects::dsl::objects.into_boxed();

        if let Some(cursor) = cursor {
            if descending_order {
                query = query.filter(objects::dsl::object_id.lt(cursor));
            } else {
                query = query.filter(objects::dsl::object_id.gt(cursor));
            }
        }

        if descending_order {
            query = query.order(objects::dsl::object_id.desc());
        } else {
            query = query.order(objects::dsl::object_id.asc());
        }

        query = query.limit(limit + 1);

        if let Some(filter) = filter {
            if let Some(object_ids) = filter.object_ids {
                query = query.filter(
                    objects::dsl::object_id.eq_any(
                        object_ids
                            .into_iter()
                            .map(|id| id.into_vec())
                            .collect::<Vec<_>>(),
                    ),
                );
            }

            if let Some(owner) = filter.owner {
                query = query.filter(objects::dsl::owner_id.eq(owner.into_vec()));

                match owner_type {
                    Some(OwnerType::Address) => {
                        query =
                            query.filter(objects::dsl::owner_type.eq(OwnerType::Address as i16));
                    }
                    Some(OwnerType::Object) => {
                        query = query.filter(objects::dsl::owner_type.eq(OwnerType::Object as i16));
                    }
                    None => {
                        query = query.filter(
                            objects::dsl::owner_type
                                .eq(OwnerType::Address as i16)
                                .or(objects::dsl::owner_type.eq(OwnerType::Object as i16)),
                        );
                    }
                    _ => Err(DbValidationError::InvalidOwnerType)?,
                }
            }

            if let Some(object_type) = filter.ty {
                query = query.filter(objects::dsl::object_type.eq(object_type));
            }
        }

        Ok(query)
    }

    fn multi_get_balances<'a>(address: Vec<u8>) -> PgBalanceQuery<'a> {
        let query = objects::dsl::objects
            .group_by(objects::dsl::coin_type)
            .select((
                diesel::dsl::sql::<diesel::sql_types::Nullable<diesel::sql_types::BigInt>>(
                    "CAST(SUM(coin_balance) AS BIGINT)",
                ),
                diesel::dsl::sql::<diesel::sql_types::Nullable<diesel::sql_types::BigInt>>(
                    "COUNT(*)",
                ),
                objects::dsl::coin_type,
            ))
            .filter(objects::dsl::owner_id.eq(address))
            .filter(objects::dsl::owner_type.eq(OwnerType::Address as i16))
            .filter(objects::dsl::coin_type.is_not_null())
            .into_boxed();

        query
    }

    fn get_balance<'a>(address: Vec<u8>, coin_type: String) -> PgBalanceQuery<'a> {
        let query = PgQueryBuilder::multi_get_balances(address);
        query.filter(objects::dsl::coin_type.eq(coin_type))
    }

    fn multi_get_checkpoints<'a>(
        cursor: Option<i64>,
        descending_order: bool,
        limit: i64,
        epoch: Option<i64>,
    ) -> checkpoints::BoxedQuery<'a, Pg> {
        let mut query = checkpoints::dsl::checkpoints.into_boxed();

        if let Some(cursor) = cursor {
            if descending_order {
                query = query.filter(checkpoints::dsl::sequence_number.lt(cursor));
            } else {
                query = query.filter(checkpoints::dsl::sequence_number.gt(cursor));
            }
        }
        if descending_order {
            query = query.order(checkpoints::dsl::sequence_number.desc());
        } else {
            query = query.order(checkpoints::dsl::sequence_number.asc());
        }
        if let Some(epoch) = epoch {
            query = query.filter(checkpoints::dsl::epoch.eq(epoch));
        }
        query = query.limit(limit + 1);

        query
    }
}

impl GenericQueryBuilder<Pg> for PgQueryBuilder {
    fn get_tx_by_digest(digest: Vec<u8>) -> transactions::BoxedQuery<'static, Pg> {
        PgQueryBuilder::get_tx_by_digest(digest)
    }
    fn get_obj(address: Vec<u8>, version: Option<i64>) -> objects::BoxedQuery<'static, Pg> {
        PgQueryBuilder::get_obj(address, version)
    }
    fn get_epoch(epoch_id: i64) -> epochs::BoxedQuery<'static, Pg> {
        PgQueryBuilder::get_epoch(epoch_id)
    }
    fn get_latest_epoch() -> epochs::BoxedQuery<'static, Pg> {
        PgQueryBuilder::get_latest_epoch()
    }
    fn get_checkpoint_by_digest(digest: Vec<u8>) -> checkpoints::BoxedQuery<'static, Pg> {
        PgQueryBuilder::get_checkpoint_by_digest(digest)
    }
    fn get_checkpoint_by_sequence_number(
        sequence_number: i64,
    ) -> checkpoints::BoxedQuery<'static, Pg> {
        PgQueryBuilder::get_checkpoint_by_sequence_number(sequence_number)
    }
    fn get_latest_checkpoint() -> checkpoints::BoxedQuery<'static, Pg> {
        PgQueryBuilder::get_latest_checkpoint()
    }
    fn multi_get_txs(
        cursor: Option<i64>,
        descending_order: bool,
        limit: i64,
        filter: Option<TransactionBlockFilter>,
        after_tx_seq_num: Option<i64>,
        before_tx_seq_num: Option<i64>,
    ) -> Result<transactions::BoxedQuery<'static, Pg>, Error> {
        PgQueryBuilder::multi_get_txs(
            cursor,
            descending_order,
            limit,
            filter,
            after_tx_seq_num,
            before_tx_seq_num,
        )
    }
    fn multi_get_coins(
        cursor: Option<Vec<u8>>,
        descending_order: bool,
        limit: i64,
        address: Vec<u8>,
        coin_type: String,
    ) -> objects::BoxedQuery<'static, Pg> {
        PgQueryBuilder::multi_get_coins(cursor, descending_order, limit, address, coin_type)
    }
    fn multi_get_objs(
        cursor: Option<Vec<u8>>,
        descending_order: bool,
        limit: i64,
        filter: Option<ObjectFilter>,
        owner_type: Option<OwnerType>,
    ) -> Result<objects::BoxedQuery<'static, Pg>, Error> {
        PgQueryBuilder::multi_get_objs(cursor, descending_order, limit, filter, owner_type)
    }
    fn multi_get_balances(address: Vec<u8>) -> BalanceQuery<'static, Pg> {
        PgQueryBuilder::multi_get_balances(address)
    }
    fn get_balance(address: Vec<u8>, coin_type: String) -> BalanceQuery<'static, Pg> {
        PgQueryBuilder::get_balance(address, coin_type)
    }
    fn multi_get_checkpoints(
        cursor: Option<i64>,
        descending_order: bool,
        limit: i64,
        epoch: Option<i64>,
    ) -> checkpoints::BoxedQuery<'static, Pg> {
        PgQueryBuilder::multi_get_checkpoints(cursor, descending_order, limit, epoch)
    }
}

pub(crate) type QueryBuilder = PgQueryBuilder;
