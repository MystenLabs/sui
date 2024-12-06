-- Write-ahead log for `sum_obj_types`.
--
-- It contains the same columns and indices as `sum_obj_types`, but with the
-- following changes:
--
-- - A `cp_sequence_number` column (and an index on it), to support pruning by
--   checkpoint.
--
-- - The primary key includes the version, as the table may contain multiple
--   versions per object ID.
--
-- - The `owner_kind` column is nullable, because this table also tracks
--   deleted and wrapped objects (where all the fields except the ID, version,
--   and checkpoint are NULL).
--
-- - There is an additional index on ID and version for querying the latest
--   version of every object.
--
-- This table is used in conjunction with `sum_obj_types` to support consistent
-- live object set queries: `sum_obj_types` holds the state of the live object
-- set at some checkpoint `C < T` where `T` is the tip of the chain, and
-- `wal_obj_types` stores all the updates and deletes between `C` and `T`.
--
-- To reconstruct the the live object set at some snapshot checkpoint `S`
-- between `C` and `T`, a query can be constructed that starts with the set
-- from `sum_obj_types` and adds updates in `wal_obj_types` from
-- `cp_sequence_number <= S`.
--
-- See `up.sql` for the original `sum_obj_types` table for documentation on
-- columns.
CREATE TABLE IF NOT EXISTS wal_obj_types
(
    object_id                   BYTEA         NOT NULL,
    object_version              BIGINT        NOT NULL,
    owner_kind                  SMALLINT,
    owner_id                    BYTEA,
    package                     BYTEA,
    module                      TEXT,
    name                        TEXT,
    instantiation               BYTEA,
    cp_sequence_number          BIGINT        NOT NULL,
    PRIMARY KEY (object_id, object_version)
);

CREATE INDEX IF NOT EXISTS wal_obj_types_cp_sequence_number
ON wal_obj_types (cp_sequence_number);

CREATE INDEX IF NOT EXISTS wal_obj_types_version
ON wal_obj_types (object_id, object_version);

CREATE INDEX IF NOT EXISTS wal_obj_types_owner
ON wal_obj_types (owner_kind, owner_id, object_id, object_version);

CREATE INDEX IF NOT EXISTS wal_obj_types_pkg
ON wal_obj_types (package, object_id, object_version);

CREATE INDEX IF NOT EXISTS wal_obj_types_mod
ON wal_obj_types (package, module, object_id, object_version);

CREATE INDEX IF NOT EXISTS wal_obj_types_name
ON wal_obj_types (package, module, name, object_id, object_version);

CREATE INDEX IF NOT EXISTS wal_obj_types_inst
ON wal_obj_types (package, module, name, instantiation, object_id, object_version);

CREATE INDEX IF NOT EXISTS wal_obj_types_owner_pkg
ON wal_obj_types (owner_kind, owner_id, package, object_id, object_version);

CREATE INDEX IF NOT EXISTS wal_obj_types_owner_mod
ON wal_obj_types (owner_kind, owner_id, package, module, object_id, object_version);

CREATE INDEX IF NOT EXISTS wal_obj_types_owner_name
ON wal_obj_types (owner_kind, owner_id, package, module, name, object_id, object_version);

CREATE INDEX IF NOT EXISTS wal_obj_types_owner_inst
ON wal_obj_types (owner_kind, owner_id, package, module, name, instantiation, object_id, object_version);
