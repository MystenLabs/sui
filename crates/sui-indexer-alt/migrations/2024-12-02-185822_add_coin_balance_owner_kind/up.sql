ALTER TABLE sum_coin_balances
ADD COLUMN coin_owner_kind SMALLINT NOT NULL DEFAULT 1;

DROP INDEX sum_coin_balances_owner_type;
CREATE INDEX sum_coin_balances_owner_type
ON sum_coin_balances (coin_owner_kind, owner_id, coin_type, coin_balance, object_id, object_version);

ALTER TABLE wal_coin_balances
ADD COLUMN coin_owner_kind SMALLINT;
UPDATE wal_coin_balances SET owner_kind = 1 WHERE owner_id IS NOT NULL;

DROP INDEX wal_coin_balances_owner_type;
CREATE INDEX wal_coin_balances_owner_type
ON wal_coin_balances (coin_owner_kind, owner_id, coin_type, coin_balance, object_id, object_version);
