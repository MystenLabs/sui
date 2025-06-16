-- A table that keeps track of all the updates to object type and owner information.
-- In particular, whenever an object's presence or ownership changes, we insert a
-- new row into this table. Each row should have a unique (object_id, cp_sequence_number)
-- pair.
-- When implementing consistency queries, we will use this table to find all
-- object IDs that match the given filters bounded by the cursor checkpoint.
-- These object IDs can then be used to look up the latest version of the objects
-- bounded by the given checkpoint in the object_versions table.
CREATE TABLE IF NOT EXISTS obj_info_v2
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
    PRIMARY KEY (cp_sequence_number, object_id)
);

CREATE INDEX IF NOT EXISTS obj_info_v2_owner_object_id_desc
ON obj_info_v2 (owner_kind, owner_id, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_v2_pkg_object_id_desc
ON obj_info_v2 (package, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_v2_mod_object_id_desc
ON obj_info_v2 (package, module, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_v2_name_object_id_desc
ON obj_info_v2 (package, module, name, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_v2_inst_object_id_desc
ON obj_info_v2 (package, module, name, instantiation, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_v2_owner_pkg_object_id_desc
ON obj_info_v2 (owner_kind, owner_id, package, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_v2_owner_mod_object_id_desc
ON obj_info_v2 (owner_kind, owner_id, package, module, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_v2_owner_name_object_id_desc
ON obj_info_v2 (owner_kind, owner_id, package, module, name, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_v2_owner_inst_object_id_desc
ON obj_info_v2 (owner_kind, owner_id, package, module, name, instantiation, cp_sequence_number DESC, object_id DESC);



-- Add the new non-null fields with default values
ALTER TABLE obj_info_v2
ADD COLUMN obsolete_at BIGINT,
ADD COLUMN marked_predecessor BOOLEAN NOT NULL DEFAULT FALSE;

-- 3. CRITICAL: T1 deletion by checkpoint range
CREATE INDEX obj_info_v2_can_delete ON obj_info_v2 (cp_sequence_number, object_id)
WHERE obsolete_at IS NOT NULL AND marked_predecessor = TRUE;

-- 4. CRITICAL: T1 deletion by obsolete_at range
CREATE INDEX obj_info_v2_obsoleted_by_range ON obj_info_v2 (obsolete_at, object_id)
WHERE obsolete_at IS NOT NULL AND marked_predecessor = TRUE;

-- For T0b: Finding unflagged predecessors efficiently
CREATE INDEX obj_info_v2_unflagged_predecessors
ON obj_info_v2 (object_id, cp_sequence_number DESC)
WHERE obsolete_at IS NULL;










-- A table that keeps track of all the updates to object type and owner information.
-- In particular, whenever an object's presence or ownership changes, we insert a
-- new row into this table. Each row should have a unique (object_id, cp_sequence_number)
-- pair.
-- When implementing consistency queries, we will use this table to find all
-- object IDs that match the given filters bounded by the cursor checkpoint.
-- These object IDs can then be used to look up the latest version of the objects
-- bounded by the given checkpoint in the object_versions table.
CREATE TABLE IF NOT EXISTS obj_info_two_tables
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
    PRIMARY KEY (cp_sequence_number, object_id)
);

CREATE INDEX IF NOT EXISTS obj_info_two_tables_owner_object_id_desc
ON obj_info_two_tables (owner_kind, owner_id, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_two_tables_pkg_object_id_desc
ON obj_info_two_tables (package, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_two_tables_mod_object_id_desc
ON obj_info_two_tables (package, module, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_two_tables_name_object_id_desc
ON obj_info_two_tables (package, module, name, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_two_tables_inst_object_id_desc
ON obj_info_two_tables (package, module, name, instantiation, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_two_tables_owner_pkg_object_id_desc
ON obj_info_two_tables (owner_kind, owner_id, package, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_two_tables_owner_mod_object_id_desc
ON obj_info_two_tables (owner_kind, owner_id, package, module, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_two_tables_owner_name_object_id_desc
ON obj_info_two_tables (owner_kind, owner_id, package, module, name, cp_sequence_number DESC, object_id DESC);

CREATE INDEX IF NOT EXISTS obj_info_two_tables_owner_inst_object_id_desc
ON obj_info_two_tables (owner_kind, owner_id, package, module, name, instantiation, cp_sequence_number DESC, object_id DESC);
