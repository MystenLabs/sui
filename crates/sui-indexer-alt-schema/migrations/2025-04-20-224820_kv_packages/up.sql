CREATE TABLE IF NOT EXISTS kv_packages
(
    package_id                  BYTEA         NOT NULL,
    package_version             BIGINT        NOT NULL,
    original_id                 BYTEA         NOT NULL,
    is_system_package           BOOLEAN       NOT NULL,
    serialized_object           BYTEA         NOT NULL,
    cp_sequence_number          BIGINT        NOT NULL,
    PRIMARY KEY (package_id, package_version)
);

CREATE INDEX IF NOT EXISTS kv_packages_cp_id_version
ON kv_packages (cp_sequence_number, original_id, package_version);

CREATE INDEX IF NOT EXISTS kv_packages_id_version_cp
ON kv_packages (original_id, package_version, cp_sequence_number);

CREATE INDEX IF NOT EXISTS kv_packages_system_packages
ON kv_packages (cp_sequence_number, original_id, package_version)
WHERE is_system_package = true;
