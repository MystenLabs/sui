ALTER TABLE watermarks
RENAME COLUMN epoch_hi TO epoch_hi_inclusive;

ALTER TABLE watermarks
RENAME COLUMN checkpoint_hi TO checkpoint_hi_inclusive;

ALTER TABLE watermarks
RENAME COLUMN tx_hi TO tx_hi_inclusive;

ALTER TABLE watermarks
RENAME COLUMN pruned_lo TO pruner_lo;
