CREATE TABLE IF NOT EXISTS kv_objects
(
    object_id                   bytea         NOT NULL,
    object_version              bigint        NOT NULL,
    serialized_object           bytea,
    PRIMARY KEY (object_id, object_version)
);
