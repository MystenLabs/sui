-- This table is used to answer queries of the form: Give me the latest version
-- of an object O with version less than or equal to V at checkpoint C. These
-- are useful for looking up dynamic fields on objects (live or historical).
CREATE TABLE IF NOT EXISTS obj_versions
(
    object_id                   BYTEA         NOT NULL,
    object_version              BIGINT        NOT NULL,
    object_digest               BYTEA         NOT NULL,
    cp_sequence_number          BIGINT        NOT NULL,
    PRIMARY KEY (object_id, object_version)
);

CREATE INDEX IF NOT EXISTS obj_versions_cp_sequence_number
ON obj_versions (cp_sequence_number);
