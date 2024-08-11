-- Your SQL goes here
CREATE TABLE objects (
    object_id                   BLOB          NOT NULL,
    object_version              BIGINT        NOT NULL,
    object_digest               BLOB          NOT NULL,
    checkpoint_sequence_number  BIGINT        NOT NULL,
    -- Immutable/Address/Object/Shared, see types.rs
    owner_type                  SMALLINT      NOT NULL,
    -- bytes of SuiAddress/ObjectID of the owner ID.
    -- Non-null for objects with an owner: Addresso or Objects
    owner_id                    BLOB,
    -- Object type
    object_type                 TEXT,
    -- Components of the StructTag: package, module, name (name of the struct, without type parameters)
    object_type_package         BLOB,
    object_type_module          TEXT,
    object_type_name            TEXT,
    -- bcs serialized Object
    serialized_object           MEDIUMBLOB    NOT NULL,
    -- Non-null when the object is a coin.
    -- e.g. `0x2::sui::SUI`
    coin_type                   TEXT,
    -- Non-null when the object is a coin.
    coin_balance                BIGINT,
    -- DynamicField/DynamicObject, see types.rs
    -- Non-null when the object is a dynamic field
    df_kind                     SMALLINT,
    -- bcs serialized DynamicFieldName
    -- Non-null when the object is a dynamic field
    df_name                     BLOB,
    -- object_type in DynamicFieldInfo.
    df_object_type              TEXT,
    -- object_id in DynamicFieldInfo.
    df_object_id                BLOB,
    CONSTRAINT objects_pk PRIMARY KEY (object_id(32))
);

-- OwnerType: 1: Address, 2: Object, see types.rs
CREATE INDEX objects_owner ON objects (owner_type, owner_id(32));
CREATE INDEX objects_coin ON objects (owner_id(32), coin_type(256));
CREATE INDEX objects_checkpoint_sequence_number ON objects (checkpoint_sequence_number);
CREATE INDEX objects_package_module_name_full_type ON objects (object_type_package(32), object_type_module(128), object_type_name(128), object_type(256));
CREATE INDEX objects_owner_package_module_name_full_type ON objects (owner_id(32), object_type_package(32), object_type_module(128), object_type_name(128), object_type(256));

-- similar to objects table, except that
-- 1. the primary key to store multiple object versions and partitions by checkpoint_sequence_number
-- 2. allow null values in some columns for deleted / wrapped objects
-- 3. object_status to mark the status of the object, which is either Active or WrappedOrDeleted
CREATE TABLE objects_history (
    object_id                   BLOB          NOT NULL,
    object_version              BIGINT        NOT NULL,
    object_status               SMALLINT      NOT NULL,
    object_digest               BLOB,
    checkpoint_sequence_number  BIGINT        NOT NULL,
    owner_type                  SMALLINT,
    owner_id                    BLOB,
    object_type                 TEXT,
    -- Components of the StructTag: package, module, name (name of the struct, without type parameters)
    object_type_package         BLOB,
    object_type_module          TEXT,
    object_type_name            TEXT,
    serialized_object           MEDIUMBLOB,
    coin_type                   TEXT,
    coin_balance                BIGINT,
    df_kind                     SMALLINT,
    df_name                     BLOB,
    df_object_type              TEXT,
    df_object_id                BLOB,
    CONSTRAINT objects_history_pk PRIMARY KEY (checkpoint_sequence_number, object_id(32), object_version)
) PARTITION BY RANGE (checkpoint_sequence_number) (
    PARTITION objects_history_partition_0 VALUES LESS THAN MAXVALUE
);
CREATE INDEX objects_history_id_version ON objects_history (object_id(32), object_version, checkpoint_sequence_number);
CREATE INDEX objects_history_owner ON objects_history (checkpoint_sequence_number, owner_type, owner_id(32));
CREATE INDEX objects_history_coin_owner ON objects_history (checkpoint_sequence_number, owner_id(32), coin_type(256), object_id(32));
CREATE INDEX objects_history_coin_only ON objects_history (checkpoint_sequence_number, coin_type(256), object_id(32));
CREATE INDEX objects_history_type ON objects_history (checkpoint_sequence_number, object_type(256));
CREATE INDEX objects_history_package_module_name_full_type ON objects_history (checkpoint_sequence_number, object_type_package(32), object_type_module(128), object_type_name(128), object_type(256));
CREATE INDEX objects_history_owner_package_module_name_full_type ON objects_history (checkpoint_sequence_number, owner_id(32), object_type_package(32), object_type_module(128), object_type_name(128), object_type(256));

-- snapshot table by folding objects_history table until certain checkpoint,
-- effectively the snapshot of objects at the same checkpoint,
-- except that it also includes deleted or wrapped objects with the corresponding object_status.
CREATE TABLE objects_snapshot (
    object_id                   BLOB          NOT NULL,
    object_version              BIGINT        NOT NULL,
    object_status               SMALLINT      NOT NULL,
    object_digest               BLOB,
    checkpoint_sequence_number  BIGINT        NOT NULL,
    owner_type                  SMALLINT,
    owner_id                    BLOB,
    object_type                 TEXT,
    object_type_package         BLOB,
    object_type_module          TEXT,
    object_type_name            TEXT,
    serialized_object           MEDIUMBLOB,
    coin_type                   TEXT,
    coin_balance                BIGINT,
    df_kind                     SMALLINT,
    df_name                     BLOB,
    df_object_type              TEXT,
    df_object_id                BLOB,
    CONSTRAINT objects_snapshot_pk PRIMARY KEY (object_id(32))
);
CREATE INDEX objects_snapshot_checkpoint_sequence_number ON objects_snapshot (checkpoint_sequence_number);
CREATE INDEX objects_snapshot_owner ON objects_snapshot (owner_type, owner_id(32), object_id(32));
CREATE INDEX objects_snapshot_coin_owner ON objects_snapshot (owner_id(32), coin_type(256), object_id(32));
CREATE INDEX objects_snapshot_coin_only ON objects_snapshot (coin_type(256), object_id(32));
CREATE INDEX objects_snapshot_package_module_name_full_type ON objects_snapshot (object_type_package(32), object_type_module(128), object_type_name(128), object_type(256));
CREATE INDEX objects_snapshot_owner_package_module_name_full_type ON objects_snapshot (owner_id(32), object_type_package(32), object_type_module(128), object_type_name(128), object_type(256));
