-- Your SQL goes here
CREATE TABLE chain_identifier
(
    checkpoint_digest   BYTEA    NOT NULL,
    PRIMARY KEY(checkpoint_digest)
);
