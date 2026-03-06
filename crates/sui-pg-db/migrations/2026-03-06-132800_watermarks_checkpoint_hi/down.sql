-- replace watermarks.checkpoint_hi with watermarks.checkpoint_hi_inclusive

ALTER TABLE watermarks ADD COLUMN checkpoint_hi_inclusive BIGINT;

UPDATE watermarks SET checkpoint_hi_inclusive = checkpoint_hi - 1;

-- delete these records because they would not have been written by the old version
DELETE FROM watermarks WHERE checkpoint_hi_inclusive = 0;

ALTER TABLE watermarks ALTER COLUMN checkpoint_hi_inclusive SET NOT NULL;

ALTER TABLE watermarks DROP COLUMN checkpoint_hi;