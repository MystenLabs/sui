CREATE TABLE display
(
    object_type     TEXT        NOT NULL,
    id              BLOB        NOT NULL,
    version         SMALLINT    NOT NULL,
    bcs             MEDIUMBLOB  NOT NULL,
    primary key (object_type(255))
);
