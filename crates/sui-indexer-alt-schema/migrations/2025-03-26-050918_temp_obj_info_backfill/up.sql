-- This is a temporary table that will be used to backfill the obj_info table.
-- It will be dropped after the backfill is complete.
CREATE TABLE IF NOT EXISTS obj_info_temp
(
    object_id                   BYTEA         NOT NULL,
    cp_sequence_number          BIGINT        NOT NULL,
    -- An enum describing the object's ownership model:
    --
    --   Immutable = 0,
    --   Address-owned = 1,
    --   Object-owned (dynamic field) = 2,
    --   Shared = 3.
    --
    -- Note that there is a distinction between an object that is owned by
    -- another object (kind 2), which relates to dynamic fields, and an object
    -- that is owned by another object's address (kind 1), which relates to
    -- transfer-to-object.
    owner_kind                  SMALLINT,
    -- The address for address-owned objects, and the parent object for
    -- object-owned objects.
    owner_id                    BYTEA,
    -- The following fields relate to the object's type. These only apply to
    -- Move Objects. For Move Packages they will all be NULL.
    --
    -- The type's package ID.
    package                     BYTEA,
    -- The type's module name.
    module                      TEXT,
    -- The type's name.
    name                        TEXT,
    -- The type's type parameters, as a BCS-encoded array of TypeTags.
    instantiation               BYTEA,
    PRIMARY KEY (object_id, cp_sequence_number)
);

CREATE INDEX IF NOT EXISTS obj_info_temp_owner
ON obj_info_temp (owner_kind, owner_id, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_temp_pkg
ON obj_info_temp (package, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_temp_mod
ON obj_info_temp (package, module, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_temp_name
ON obj_info_temp (package, module, name, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_temp_inst
ON obj_info_temp (package, module, name, instantiation, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_temp_owner_pkg
ON obj_info_temp (owner_kind, owner_id, package, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_temp_owner_mod
ON obj_info_temp (owner_kind, owner_id, package, module, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_temp_owner_name
ON obj_info_temp (owner_kind, owner_id, package, module, name, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_temp_owner_inst
ON obj_info_temp (owner_kind, owner_id, package, module, name, instantiation, cp_sequence_number DESC, object_id);
