// @generated automatically by Diesel CLI.
//

diesel::table! {
    events (id) {
        id -> Int8,
        transaction_digest -> Varchar,
        event_sequence -> Int8,
        sender -> Varchar,
        package -> Varchar,
        module -> Text,
        event_type -> Text,
        event_time_ms -> Nullable<Int8>,
        event_bcs -> Bytea,
    }
}
