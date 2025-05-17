DROP INDEX IF EXISTS kv_packages_id_cp_version;

CREATE INDEX IF NOT EXISTS kv_packages_id_version_cp
ON kv_packages (original_id, package_version, cp_sequence_number);

CREATE INDEX IF NOT EXISTS kv_packages_system_packages
ON kv_packages (cp_sequence_number, original_id, package_version)
WHERE is_system_package = true;
