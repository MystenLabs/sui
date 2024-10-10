ALTER TABLE watermarks
RENAME COLUMN epoch_hi_inclusive TO epoch_hi;

ALTER TABLE watermarks
RENAME COLUMN checkpoint_hi_inclusive TO checkpoint_hi;

ALTER TABLE watermarks
RENAME COLUMN tx_hi_inclusive TO tx_hi;

ALTER TABLE watermarks
RENAME COLUMN pruner_lo TO pruned_lo;
