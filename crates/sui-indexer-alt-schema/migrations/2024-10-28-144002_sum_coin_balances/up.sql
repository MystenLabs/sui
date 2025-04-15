-- A summary table for coins owned by addresses
--
-- This can be used to paginate the coin balances of a given address at an
-- instant in time, returning coins in descending balance order.
CREATE TABLE IF NOT EXISTS sum_coin_balances
(
    object_id                   BYTEA         PRIMARY KEY,
    object_version              BIGINT        NOT NULL,
    -- The address that owns this version of the coin (it is guaranteed to be
    -- address-owned).
    owner_id                    BYTEA         NOT NULL,
    -- The type of the coin, as a BCS-serialized `TypeTag`. This is only the
    -- marker type, and not the full object type (e.g. `0x0...02::sui::SUI`).
    coin_type                   BYTEA         NOT NULL,
    -- The balance of the coin at this version.
    coin_balance                BIGINT        NOT NULL
);

CREATE INDEX IF NOT EXISTS sum_coin_balances_owner_type
ON sum_coin_balances (owner_id, coin_type, coin_balance, object_id, object_version);
