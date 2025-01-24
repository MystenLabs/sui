-- A table of coin balance buckets, keyed on object ID and checkpoint sequence number.
-- At the end of each checkpoint, we insert a row for each coin balance bucket, if it has changed.
-- We bucketize coin balances to reduce the number of distinct values and help with both write and
-- read performance. Bucket is calculated as floor(log10(coin_balance)).
-- We also keep a record when we delete or wrap an object, which we would need for consistency query.
-- All fields except the primary key will be `NULL` for delete/wrap records.
CREATE TABLE IF NOT EXISTS coin_balance_buckets
(
    object_id                   BYTEA         NOT NULL,
    cp_sequence_number          BIGINT        NOT NULL,
    -- The kind of owner of this coin. We need this to support ConsensusV2 objects.
    -- A coin can be either owned by an address through fast-path ownership, or
    -- by an anddress through ConsensusV2 ownership.
    -- This is represented by `StoredCoinOwnerKind` in `models/objects.rs`, which is different
    -- from `StoredOwnerKind` used in `obj_info` table.
    owner_kind                  SMALLINT,
    -- The address that owns this version of the coin (it is guaranteed to be
    -- address-owned).
    owner_id                    BYTEA,
    -- The type of the coin, as a BCS-serialized `TypeTag`. This is only the
    -- marker type, and not the full object type (e.g. `0x0...02::sui::SUI`).
    coin_type                   BYTEA,
    -- The balance bucket of the coin, which is log10(coin_balance).
    coin_balance_bucket         SMALLINT,
    PRIMARY KEY (object_id, cp_sequence_number)
);

CREATE INDEX IF NOT EXISTS coin_balances_buckets_owner_type
ON coin_balance_buckets (owner_kind, owner_id, coin_type, coin_balance_bucket DESC, cp_sequence_number DESC, object_id);
