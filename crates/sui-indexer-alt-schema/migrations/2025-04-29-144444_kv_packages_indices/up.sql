DROP INDEX IF EXISTS
    kv_packages_id_version_cp,
    kv_packages_system_packages;

CREATE INDEX IF NOT EXISTS kv_packages_id_cp_version
ON kv_packages (original_id, cp_sequence_number DESC, package_version DESC);

CREATE INDEX kv_packages_system_packages
ON kv_packages (original_id, cp_sequence_number DESC, package_version DESC)
WHERE is_system_package = true;
