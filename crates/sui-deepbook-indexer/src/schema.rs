// @generated automatically by Diesel CLI.

diesel::table! {
    flashloans (digest) {
        digest -> Text,
        sender -> Text,
        checkpoint -> Int8,
        timestamp -> Timestamp,
        borrow -> Bool,
        pool_id -> Text,
        borrow_quantity -> Int8,
        type_name -> Text,
    }
}

diesel::table! {
    order_fills (digest) {
        digest -> Text,
        sender -> Text,
        checkpoint -> Int8,
        timestamp -> Timestamp,
        pool_id -> Text,
        maker_order_id -> Numeric,
        taker_order_id -> Numeric,
        maker_client_order_id -> Int8,
        taker_client_order_id -> Int8,
        price -> Int8,
        taker_is_bid -> Bool,
        base_quantity -> Int8,
        quote_quantity -> Int8,
        maker_balance_manager_id -> Text,
        taker_balance_manager_id -> Text,
        onchain_timestamp -> Int8,
    }
}

diesel::table! {
    order_updates (digest) {
        digest -> Text,
        sender -> Text,
        checkpoint -> Int8,
        timestamp -> Timestamp,
        status -> Text,
        pool_id -> Text,
        order_id -> Numeric,
        client_order_id -> Int8,
        price -> Int8,
        is_bid -> Bool,
        quantity -> Int8,
        onchain_timestamp -> Int8,
        balance_manager_id -> Text,
        trader -> Text,
    }
}

diesel::table! {
    pool_prices (digest) {
        digest -> Text,
        sender -> Text,
        checkpoint -> Int8,
        timestamp -> Timestamp,
        target_pool -> Text,
        reference_pool -> Text,
        conversion_rate -> Int8,
    }
}

diesel::table! {
    progress_store (task_name) {
        task_name -> Text,
        checkpoint -> Int8,
        target_checkpoint -> Int8,
        timestamp -> Nullable<Timestamp>,
    }
}

diesel::table! {
    sui_error_transactions (txn_digest) {
        txn_digest -> Text,
        sender_address -> Text,
        timestamp_ms -> Int8,
        failure_status -> Text,
        cmd_idx -> Nullable<Int8>,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    flashloans,
    order_fills,
    order_updates,
    pool_prices,
    progress_store,
    sui_error_transactions,
);
