ALTER TABLE watermarks
    ALTER COLUMN reader_lo SET NOT NULL,
    ALTER COLUMN pruner_timestamp SET NOT NULL,
    ALTER COLUMN pruner_hi SET NOT NULL;
