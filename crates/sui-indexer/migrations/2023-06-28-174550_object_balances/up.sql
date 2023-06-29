-- Your SQL goes here

CREATE TABLE object_balances
(
    id          address NOT NULL,
    version     BIGINT  NOT NULL,
    coin_type   VARCHAR NOT NULL,
    balance     BIGINT  NOT NULL,

    CONSTRAINT object_balances_pk PRIMARY KEY (id, version, coin_type)
);
