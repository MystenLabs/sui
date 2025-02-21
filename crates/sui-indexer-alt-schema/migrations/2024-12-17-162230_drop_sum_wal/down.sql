CREATE TABLE IF NOT EXISTS sum_obj_types
(
    object_id                   BYTEA         PRIMARY KEY,
    object_version              BIGINT        NOT NULL,
    owner_kind                  SMALLINT      NOT NULL,
    owner_id                    BYTEA,
    package                     BYTEA,
    module                      TEXT,
    name                        TEXT,
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

CREATE TABLE IF NOT EXISTS sum_coin_balances
(
    object_id                   BYTEA         PRIMARY KEY,
    object_version              BIGINT        NOT NULL,
    owner_id                    BYTEA         NOT NULL,
    coin_type                   BYTEA         NOT NULL,
    coin_balance                BIGINT        NOT NULL
);

CREATE INDEX IF NOT EXISTS sum_coin_balances_owner_type
ON sum_coin_balances (owner_id, coin_type, coin_balance, object_id, object_version);
