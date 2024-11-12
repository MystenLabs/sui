-- Write-ahead log for `sum_coin_balances`.
--
-- It contains the same columns and indices as `sum_coin_balances`, but with
-- the following changes:
--
-- - A `cp_sequence_number` column (and an index on it), to support pruning by
--   checkpoint.
--
-- - The primary key includes the version, as the table may contain multiple
--   versions per object ID.
--
-- - The other fields are nullable, because this table also tracks deleted and
--   wrapped objects.
--
-- - There is an additional index on ID and version for querying the latest
--   version of every object.
--
-- This table is used in conjunction with `sum_coin_balances` to support
-- consistent live object set queries: `sum_coin_balances` holds the state of
-- the live object set at some checkpoint `C < T` where `T` is the tip of the
-- chain, and `wal_coin_balances` stores all the updates and deletes between
-- `C` and `T`.
--
-- To reconstruct the the live object set at some snapshot checkpoint `S`
-- between `C` and `T`, a query can be constructed that starts with the set
-- from `sum_coin_balances` and adds updates in `wal_coin_balances` from
-- `cp_sequence_number <= S`.
--
-- See `up.sql` for the original `sum_coin_balances` table for documentation on
-- columns.
CREATE TABLE IF NOT EXISTS wal_coin_balances
(
    object_id                   BYTEA         NOT NULL,
    object_version              BIGINT        NOT NULL,
    owner_id                    BYTEA,
    coin_type                   BYTEA,
    coin_balance                BIGINT,
    cp_sequence_number          BIGINT        NOT NULL,
    PRIMARY KEY (object_id, object_version)
);

CREATE INDEX IF NOT EXISTS wal_coin_balances_cp_sequence_number
ON wal_coin_balances (cp_sequence_number);

CREATE INDEX IF NOT EXISTS wal_coin_balances_version
ON wal_coin_balances (object_id, object_version);

CREATE INDEX IF NOT EXISTS wal_coin_balances_owner_type
ON wal_coin_balances (owner_id, coin_type, coin_balance, object_id, object_version);
