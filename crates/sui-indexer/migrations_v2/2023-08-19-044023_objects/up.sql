CREATE TABLE objects (
    object_id                   bytea         PRIMARY KEY,
    object_version              bigint        NOT NULL,
    object_digest               bytea         NOT NULL,
    checkpoint_sequence_number  bigint        NOT NULL,
    owner_type                  smallint      NOT NULL,
    -- only non-null for objects with an owner,
    -- the owner can be an account or an object. 
    owner_id                    bytea,
    serialized_object           bytea         NOT NULL,
    coin_type                   text,
    coin_balance                bigint,
    df_kind                     smallint,
    df_name                     bytea,
    df_object_type              text,
    df_object_id                bytea
);

-- 1: Address, 2: Object, see types_v2.rs
CREATE INDEX objects_owner ON objects (owner_type, owner_id) WHERE owner_type BETWEEN 1 AND 2 AND owner_id IS NOT NULL;
CREATE INDEX objects_coin ON objects USING HASH (coin_type) WHERE coin_type IS NOT NULL;
