CREATE TABLE IF NOT EXISTS objects_version_unpartitioned (
    object_id           bytea         NOT NULL,
    object_version      bigint        NOT NULL,
    cp_sequence_number  bigint        NOT NULL,
    PRIMARY KEY (object_id, object_version)
);
