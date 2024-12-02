ALTER TABLE sum_coin_balances
ADD COLUMN owner_kind SMALLINT NOT NULL DEFAULT 1;

ALTER TABLE wal_coin_balances
ADD COLUMN owner_kind SMALLINT;
UPDATE wal_coin_balances SET owner_kind = 1 WHERE owner_id IS NOT NULL;
