-- A table that keeps track of all the updates to object type and owner information.
-- In particular, whenever an object's presence or ownership changes, we insert a
-- new row into this table. Each row should have a unique (object_id, cp_sequence_number)
-- pair.
-- When implementing consistency queries, we will use this table to find all
-- object IDs that match the given filters bounded by the cursor checkpoint.
-- These object IDs can then be used to look up the latest version of the objects
-- bounded by the given checkpoint in the object_versions table.
CREATE TABLE IF NOT EXISTS obj_info
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

CREATE INDEX IF NOT EXISTS obj_info_owner
ON obj_info (owner_kind, owner_id, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_pkg
ON obj_info (package, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_mod
ON obj_info (package, module, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_name
ON obj_info (package, module, name, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_inst
ON obj_info (package, module, name, instantiation, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_owner_pkg
ON obj_info (owner_kind, owner_id, package, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_owner_mod
ON obj_info (owner_kind, owner_id, package, module, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_owner_name
ON obj_info (owner_kind, owner_id, package, module, name, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_owner_inst
ON obj_info (owner_kind, owner_id, package, module, name, instantiation, cp_sequence_number DESC, object_id);
