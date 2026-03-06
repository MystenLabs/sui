-- replace watermarks.checkpoint_hi_inclusive with watermarks.checkpoint_hi

ALTER TABLE watermarks ADD COLUMN checkpoint_hi BIGINT;

UPDATE watermarks SET checkpoint_hi = checkpoint_hi_inclusive + 1;

ALTER TABLE watermarks ALTER COLUMN checkpoint_hi SET NOT NULL;

ALTER TABLE watermarks DROP COLUMN checkpoint_hi_inclusive;