CREATE INDEX CONCURRENTLY IF NOT EXISTS wal_coin_balances_owner_kind_owner_id_type
ON wal_coin_balances (coin_owner_kind, owner_id, coin_type, coin_balance, object_id, object_version);
