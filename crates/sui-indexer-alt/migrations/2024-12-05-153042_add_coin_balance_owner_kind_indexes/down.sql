CREATE INDEX CONCURRENTLY IF NOT EXISTS sum_coin_balances_owner_type
ON sum_coin_balances (owner_id, coin_type, coin_balance, object_id, object_version);
