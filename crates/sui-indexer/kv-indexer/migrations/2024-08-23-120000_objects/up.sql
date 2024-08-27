CREATE TABLE objects (
    object_id                   bytea         NOT NULL,
    object_version              bigint        NOT NULL,
    -- Null indicates that the object at this version is either deleted or wrapped.
    serialized_object           bytea,
    PRIMARY KEY (object_id, object_version)
);
