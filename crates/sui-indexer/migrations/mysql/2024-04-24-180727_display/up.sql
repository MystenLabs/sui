-- Your SQL goes here
CREATE TABLE display
(
    object_type     text        NOT NULL,
    id              BLOB       NOT NULL,
    version         SMALLINT    NOT NULL,
    bcs             BLOB       NOT NULL,
    primary key (object_type(255))
);
