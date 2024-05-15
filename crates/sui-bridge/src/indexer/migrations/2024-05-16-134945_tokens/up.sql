-- Your SQL goes here
CREATE TABLE tokens
(
    message_key                 bytea        PRIMARY KEY,
    checkpoint                  bigint       NOT NULL,
    epoch                       bigint       NOT NULL,
    token_type                  int          NOT NULL,
    source_chain                int          NOT NULL,
    destination_chain           int          NOT NULL
);