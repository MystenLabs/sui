ALTER TABLE checkpoints
    ADD COLUMN validator_signature TEXT NOT NULL DEFAULT '';
