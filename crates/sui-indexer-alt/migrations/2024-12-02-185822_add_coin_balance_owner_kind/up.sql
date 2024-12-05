ALTER TABLE sum_coin_balances
ADD COLUMN coin_owner_kind SMALLINT NOT NULL DEFAULT 1;

ALTER TABLE wal_coin_balances
ADD COLUMN coin_owner_kind SMALLINT;
UPDATE wal_coin_balances SET coin_owner_kind = 1 WHERE owner_id IS NOT NULL;
