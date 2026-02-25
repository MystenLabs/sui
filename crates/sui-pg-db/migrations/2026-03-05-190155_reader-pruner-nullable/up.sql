ALTER TABLE watermarks
    ALTER COLUMN reader_lo DROP NOT NULL,
    ALTER COLUMN pruner_timestamp DROP NOT NULL,
    ALTER COLUMN pruner_hi DROP NOT NULL;
