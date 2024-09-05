-- This table will store every history version of each object, and never get pruned.
-- Since it can grow indefinitely, we keep minimum amount of information in this table for the purpose
-- of point lookups.
CREATE TABLE full_objects_history
(
    object_id                   bytea         NOT NULL,
    object_version              bigint        NOT NULL,
    serialized_object           bytea,
    PRIMARY KEY (object_id, object_version)
);
