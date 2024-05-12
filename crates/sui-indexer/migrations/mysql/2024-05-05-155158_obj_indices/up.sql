-- The Postgres version of this table is partitioned by the first byte
-- of object_id, but this kind of partition is not easily supported in
-- MySQL, so this variant is unpartitioned for now.
CREATE TABLE objects_version (
    object_id           blob          NOT NULL,
    object_version      bigint        NOT NULL,
    cp_sequence_number  bigint        NOT NULL,
    PRIMARY KEY (object_id(32), object_version)
)
