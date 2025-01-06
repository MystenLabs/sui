CREATE TABLE IF NOT EXISTS sum_packages
(
    package_id                  BYTEA         PRIMARY KEY,
    original_id                 BYTEA         NOT NULL,
    package_version             BIGINT        NOT NULL,
    move_package                BYTEA         NOT NULL,
    cp_sequence_number          BIGINT        NOT NULL
);

CREATE INDEX IF NOT EXISTS sum_packages_cp_id_version
ON sum_packages (cp_sequence_number, original_id, package_version);

CREATE INDEX IF NOT EXISTS sum_packages_id_version_cp
ON sum_packages (original_id, package_version, cp_sequence_number);
