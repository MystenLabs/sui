CREATE TABLE packages
(
    package_id                   BLOB          NOT NULL,
    original_id                  BLOB          NOT NULL,
    package_version              BIGINT        NOT NULL,
    -- bcs serialized MovePackage
    move_package                 MEDIUMBLOB    NOT NULL,
    checkpoint_sequence_number   BIGINT        NOT NULL,
    CONSTRAINT packages_pk PRIMARY KEY (package_id(32), original_id(32), package_version),
    CONSTRAINT packages_unique_package_id UNIQUE (package_id(32))
);

CREATE INDEX packages_cp_id_version ON packages (checkpoint_sequence_number, original_id(32), package_version);
CREATE INDEX packages_id_version_cp ON packages (original_id(32), package_version, checkpoint_sequence_number);
