DROP INDEX CONCURRENTLY IF EXISTS sum_coin_balances_owner_kind_owner_id_type;
CREATE INDEX CONCURRENTLY IF NOT EXISTS sum_coin_balances_owner_type
ON sum_coin_balances (owner_id, coin_type, coin_balance, object_id, object_version);

DROP INDEX CONCURRENTLY IF EXISTS wal_coin_balances_owner_kind_owner_id_type;
CREATE INDEX CONCURRENTLY IF NOT EXISTS wal_coin_balances_owner_type
ON wal_coin_balances (owner_id, coin_type, coin_balance, object_id, object_version);
