CREATE TABLE objects (
    object_id                   bytea         PRIMARY KEY,
    object_version              bigint        NOT NULL,
    object_digest               bytea         NOT NULL,
    checkpoint_sequence_number  bigint        NOT NULL,
    -- Immutable/Address/Object/Shared, see types_v2.rs
    owner_type                  smallint      NOT NULL,
    -- bytes of SuiAddress/ObjectID of the owner ID.
    -- Non-null for objects with an owner: Addresso or Objects
    owner_id                    bytea,
    -- Object type
    object_type                 text,
    -- bcs serialized Object
    serialized_object           bytea         NOT NULL,
    -- Non-null when the object is a coin.
    -- e.g. `0x2::sui::SUI`
    coin_type                   text,
    -- Non-null when the object is a coin.
    coin_balance                bigint,
    -- DynamicField/DynamicObject, see types_v2.rs
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

-- OwnerType: 1: Address, 2: Object, see types_v2.rs
CREATE INDEX objects_owner ON objects (owner_type, owner_id) WHERE owner_type BETWEEN 1 AND 2 AND owner_id IS NOT NULL;
CREATE INDEX objects_coin ON objects (owner_id, coin_type) WHERE coin_type IS NOT NULL AND owner_type = 1;
CREATE INDEX objects_checkpoint_sequence_number ON objects (checkpoint_sequence_number);
CREATE INDEX objects_type ON objects (object_type);
