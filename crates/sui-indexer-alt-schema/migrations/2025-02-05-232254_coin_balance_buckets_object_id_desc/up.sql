DROP INDEX IF EXISTS coin_balances_buckets_owner_type;

CREATE INDEX IF NOT EXISTS coin_balances_buckets_object_id_desc
ON coin_balance_buckets (owner_kind, owner_id, coin_type, coin_balance_bucket DESC, cp_sequence_number DESC, object_id DESC);
