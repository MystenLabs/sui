CREATE TABLE packages
(
    package_id                   bytea          PRIMARY KEY,
    original_id                  bytea          NOT NULL,
    package_version              bigint         NOT NULL,
    -- bcs serialized MovePackage
    move_package                 bytea          NOT NULL,
    checkpoint_sequence_number   bigint         NOT NULL
);

CREATE INDEX packages_cp_id_version ON packages (checkpoint_sequence_number, original_id, package_version);
CREATE INDEX packages_id_version_cp ON packages (original_id, package_version, checkpoint_sequence_number);
