-- Your SQL goes here
CREATE TABLE IF NOT EXISTS order_modified
(
    digest                      TEXT         PRIMARY KEY,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    pool_id                     TEXT         NOT NULL,
    order_id                    TEXT         NOT NULL,
    client_order_id             TEXT         NOT NULL,
    price                       TEXT         NOT NULL,
    is_bid                      BOOLEAN      NOT NULL,
    new_quantity                TEXT         NOT NULL,
    onchain_timestamp           TEXT         NOT NULL
);

CREATE TABLE IF NOT EXISTS order_placed
(
    digest                      TEXT         PRIMARY KEY,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    balance_manager_id          TEXT         NOT NULL,
    pool_id                     TEXT         NOT NULL,
    order_id                    TEXT         NOT NULL,
    client_order_id             TEXT         NOT NULL,
    trader                      TEXT         NOT NULL,
    price                       TEXT         NOT NULL,
    is_bid                      BOOLEAN      NOT NULL,
    placed_quantity             TEXT         NOT NULL,
    expire_timestamp            TEXT         NOT NULL
);

CREATE TABLE IF NOT EXISTS flashloan
(
    digest                      TEXT         PRIMARY KEY,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    borrow                      BOOLEAN      NOT NULL,
    pool_id                     TEXT         NOT NULL,
    borrow_quantity             TEXT         NOT NULL,
    type_name                   TEXT         NOT NULL
);