CREATE TABLE display
(
    object_type     text        PRIMARY KEY,
    id              BYTEA       NOT NULL,
    version         SMALLINT    NOT NULL,
    bcs             BYTEA       NOT NULL
);
