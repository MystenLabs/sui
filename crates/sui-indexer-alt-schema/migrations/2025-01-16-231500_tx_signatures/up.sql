ALTER TABLE kv_transactions
ADD COLUMN IF NOT EXISTS user_signatures BYTEA NOT NULL;
