-- Your SQL goes here

CREATE TABLE watermarks
(
    name        VARCHAR PRIMARY KEY NOT NULL,
    checkpoint  BIGINT,
    epoch       BIGINT
);
