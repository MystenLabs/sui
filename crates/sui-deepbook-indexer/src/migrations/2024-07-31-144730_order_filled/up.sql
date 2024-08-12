-- Your SQL goes here
CREATE TABLE IF NOT EXISTS order_filled
(
    digest                      TEXT         PRIMARY KEY,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    base_quantity               TEXT         NOT NULL,
    maker_balance_manager_id    TEXT         NOT NULL,
    maker_client_order_id       TEXT         NOT NULL,
    maker_order_id              TEXT         NOT NULL,
    pool_id                     TEXT         NOT NULL,
    price                       TEXT         NOT NULL,
    taker_balance_manager_id    TEXT         NOT NULL,
    taker_client_order_id       TEXT         NOT NULL,
    taker_is_bid                BOOLEAN      NOT NULL,
    taker_order_id              TEXT         NOT NULL,
    onchain_timestamp           TEXT         NOT NULL
);