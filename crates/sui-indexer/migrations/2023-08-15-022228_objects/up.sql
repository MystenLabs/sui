CREATE TABLE objects (
    object_id                   bytea         PRIMARY KEY,
    object_version              bigint        NOT NULL,
    object_digest               bytea         NOT NULL,
    checkpoint_sequence_number  bigint        NOT NULL,
    object_status               smallint      NOT NULL,
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

CREATE INDEX objects_owner ON objects (owner_type, owner_id);
CREATE INDEX objects_coin ON objects USING HASH (coin_type);
