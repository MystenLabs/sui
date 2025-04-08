-- A summary table of live objects, with owner and type information
--
-- This can be used to paginate the live object set at an instant in time,
-- filtering by a combination of owner and/or type.
CREATE TABLE IF NOT EXISTS sum_obj_types
(
    object_id                   BYTEA         PRIMARY KEY,
    object_version              BIGINT        NOT NULL,
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
    owner_kind                  SMALLINT      NOT NULL,
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
    instantiation               BYTEA
);

CREATE INDEX IF NOT EXISTS sum_obj_types_owner
ON sum_obj_types (owner_kind, owner_id, object_id, object_version);

CREATE INDEX IF NOT EXISTS sum_obj_types_pkg
ON sum_obj_types (package, object_id, object_version);

CREATE INDEX IF NOT EXISTS sum_obj_types_mod
ON sum_obj_types (package, module, object_id, object_version);

CREATE INDEX IF NOT EXISTS sum_obj_types_name
ON sum_obj_types (package, module, name, object_id, object_version);

CREATE INDEX IF NOT EXISTS sum_obj_types_inst
ON sum_obj_types (package, module, name, instantiation, object_id, object_version);

CREATE INDEX IF NOT EXISTS sum_obj_types_owner_pkg
ON sum_obj_types (owner_kind, owner_id, package, object_id, object_version);

CREATE INDEX IF NOT EXISTS sum_obj_types_owner_mod
ON sum_obj_types (owner_kind, owner_id, package, module, object_id, object_version);

CREATE INDEX IF NOT EXISTS sum_obj_types_owner_name
ON sum_obj_types (owner_kind, owner_id, package, module, name, object_id, object_version);

CREATE INDEX IF NOT EXISTS sum_obj_types_owner_inst
ON sum_obj_types (owner_kind, owner_id, package, module, name, instantiation, object_id, object_version);
