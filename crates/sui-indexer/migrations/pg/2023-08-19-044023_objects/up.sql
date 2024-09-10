CREATE TABLE objects (
    object_id                   bytea         PRIMARY KEY,
    object_version              bigint        NOT NULL,
    object_digest               bytea         NOT NULL,
    checkpoint_sequence_number  bigint        NOT NULL,
    -- Immutable/Address/Object/Shared, see types.rs
    owner_type                  smallint      NOT NULL,
    -- bytes of SuiAddress/ObjectID of the owner ID.
    -- Non-null for objects with an owner: Addresso or Objects
    owner_id                    bytea,
    -- Object type
    object_type                 text,
    -- Components of the StructTag: package, module, name (name of the struct, without type parameters)
    object_type_package         bytea,
    object_type_module          text,
    object_type_name            text,
    -- bcs serialized Object
    serialized_object           bytea         NOT NULL,
    -- Non-null when the object is a coin.
    -- e.g. `0x2::sui::SUI`
    coin_type                   text,
    -- Non-null when the object is a coin.
    coin_balance                bigint,
    -- DynamicField/DynamicObject, see types.rs
    -- Non-null when the object is a dynamic field
    df_kind                     smallint,
    -- bcs serialized DynamicFieldName
    -- Non-null when the object is a dynamic field
    df_name                     bytea,
    -- object_type in DynamicFieldInfo.
    df_object_type              text,
    -- object_id in DynamicFieldInfo.
    df_object_id                bytea
);

-- OwnerType: 1: Address, 2: Object, see types.rs
CREATE INDEX objects_owner ON objects (owner_type, owner_id) WHERE owner_type BETWEEN 1 AND 2 AND owner_id IS NOT NULL;
CREATE INDEX objects_coin ON objects (owner_id, coin_type) WHERE coin_type IS NOT NULL AND owner_type = 1;
CREATE INDEX objects_checkpoint_sequence_number ON objects (checkpoint_sequence_number);
CREATE INDEX objects_package_module_name_full_type ON objects (object_type_package, object_type_module, object_type_name, object_type);
CREATE INDEX objects_owner_package_module_name_full_type ON objects (owner_id, object_type_package, object_type_module, object_type_name, object_type);

-- similar to objects table, except that
-- 1. the primary key to store multiple object versions and partitions by checkpoint_sequence_number
-- 2. allow null values in some columns for deleted / wrapped objects
-- 3. object_status to mark the status of the object, which is either Active or WrappedOrDeleted
CREATE TABLE objects_history (
    object_id                   bytea         NOT NULL,
    object_version              bigint        NOT NULL,
    object_status               smallint      NOT NULL,
    object_digest               bytea,
    checkpoint_sequence_number  bigint        NOT NULL,
    owner_type                  smallint,
    owner_id                    bytea,
    object_type                 text,
    object_type_package         bytea,
    object_type_module          text,
    object_type_name            text,
    serialized_object           bytea,
    coin_type                   text,
    coin_balance                bigint,
    df_kind                     smallint,
    df_name                     bytea,
    df_object_type              text,
    df_object_id                bytea,
    CONSTRAINT objects_history_pk PRIMARY KEY (checkpoint_sequence_number, object_id, object_version)
) PARTITION BY RANGE (checkpoint_sequence_number);
CREATE INDEX objects_history_id_version ON objects_history (object_id, object_version, checkpoint_sequence_number);
CREATE INDEX objects_history_owner ON objects_history (checkpoint_sequence_number, owner_type, owner_id) WHERE owner_type BETWEEN 1 AND 2 AND owner_id IS NOT NULL;
CREATE INDEX objects_history_coin_owner ON objects_history (checkpoint_sequence_number, owner_id, coin_type, object_id) WHERE coin_type IS NOT NULL AND owner_type = 1;
CREATE INDEX objects_history_coin_only ON objects_history (checkpoint_sequence_number, coin_type, object_id) WHERE coin_type IS NOT NULL;
CREATE INDEX objects_history_type ON objects_history (checkpoint_sequence_number, object_type);
CREATE INDEX objects_history_package_module_name_full_type ON objects_history (checkpoint_sequence_number, object_type_package, object_type_module, object_type_name, object_type);
CREATE INDEX objects_history_owner_package_module_name_full_type ON objects_history (checkpoint_sequence_number, owner_id, object_type_package, object_type_module, object_type_name, object_type);
-- init with first partition of the history table
CREATE TABLE objects_history_partition_0 PARTITION OF objects_history FOR VALUES FROM (0) TO (MAXVALUE);

-- snapshot table by folding objects_history table until certain checkpoint,
-- effectively the snapshot of objects at the same checkpoint,
-- except that it also includes deleted or wrapped objects with the corresponding object_status.
CREATE TABLE objects_snapshot (
    object_id                   bytea         PRIMARY KEY,
    object_version              bigint        NOT NULL,
    object_status               smallint      NOT NULL,
    object_digest               bytea,
    checkpoint_sequence_number  bigint        NOT NULL,
    owner_type                  smallint,
    owner_id                    bytea,
    object_type                 text,
    object_type_package         bytea,
    object_type_module          text,
    object_type_name            text,
    serialized_object           bytea,
    coin_type                   text,
    coin_balance                bigint,
    df_kind                     smallint,
    df_name                     bytea,
    df_object_type              text,
    df_object_id                bytea
);
CREATE INDEX objects_snapshot_checkpoint_sequence_number ON objects_snapshot (checkpoint_sequence_number);
CREATE INDEX objects_snapshot_owner ON objects_snapshot (owner_type, owner_id, object_id) WHERE owner_type BETWEEN 1 AND 2 AND owner_id IS NOT NULL;
CREATE INDEX objects_snapshot_coin_owner ON objects_snapshot (owner_id, coin_type, object_id) WHERE coin_type IS NOT NULL AND owner_type = 1;
CREATE INDEX objects_snapshot_coin_only ON objects_snapshot (coin_type, object_id) WHERE coin_type IS NOT NULL;
CREATE INDEX objects_snapshot_type_id ON objects_snapshot (object_type_package, object_type_module, object_type_name, object_type, object_id);
CREATE INDEX objects_snapshot_id_type ON objects_snapshot (object_id, object_type_package, object_type_module, object_type_name, object_type);
CREATE INDEX objects_snapshot_owner_package_module_name_full_type ON objects_snapshot (owner_id, object_type_package, object_type_module, object_type_name, object_type);
