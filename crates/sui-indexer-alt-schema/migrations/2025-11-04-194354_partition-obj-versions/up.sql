DROP TABLE IF EXISTS obj_versions;

CREATE TABLE obj_versions
(
    object_id          BYTEA  NOT NULL,
    object_version     BIGINT NOT NULL,
    object_digest      BYTEA,
    cp_sequence_number BIGINT NOT NULL,
    PRIMARY KEY (object_id, object_version)
) PARTITION BY HASH (object_id);

-- Create 16 partitions
DO $$
DECLARE
    num_partitions INT := 32;
    i INT;
BEGIN
    FOR i IN 0..(num_partitions - 1) LOOP
        EXECUTE format(
            'CREATE TABLE obj_versions_p%s PARTITION OF obj_versions
             FOR VALUES WITH (MODULUS %s, REMAINDER %s)',
            i, num_partitions, i
        );
    END LOOP;
END $$;

-- Create indexes (automatically applied to all partitions)
CREATE INDEX obj_versions_cp_sequence_number
ON obj_versions (cp_sequence_number);

CREATE INDEX obj_versions_id_cp_version
ON obj_versions (object_id, cp_sequence_number DESC, object_version DESC);
