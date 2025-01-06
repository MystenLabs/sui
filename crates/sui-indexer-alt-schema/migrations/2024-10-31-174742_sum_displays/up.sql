-- This table tracks the latest versions of `Display`, keyed by Object type.
CREATE TABLE IF NOT EXISTS sum_displays
(
    -- BCS-encoded StructTag of the object that this Display belongs to.
    object_type                 BYTEA         PRIMARY KEY,
    -- Object ID of the Display object
    display_id                  BYTEA         NOT NULL,
    -- Version of the Display object (In the VersionUpdate event this is stored as a u16)
    display_version             SMALLINT      NOT NULL,
    -- BCS-encoded content of DisplayVersionUpdatedEvent that was indexed into
    -- this record.
    display                     BYTEA         NOT NULL
);
