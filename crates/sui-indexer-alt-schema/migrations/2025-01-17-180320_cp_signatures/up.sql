ALTER TABLE kv_checkpoints
ADD COLUMN IF NOT EXISTS checkpoint_summary BYTEA NOT NULL,
ADD COLUMN IF NOT EXISTS validator_signatures BYTEA NOT NULL,
DROP COLUMN IF EXISTS certified_checkpoint;
