-- Your SQL goes here
CREATE TABLE IF NOT EXISTS order_canceled
(
    digest                          TEXT         PRIMARY KEY,
    sender                          TEXT         NOT NULL,
    checkpoint                      BIGINT       NOT NULL,
    timestamp                       TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    pool_id                         TEXT         NOT NULL,
    order_id                        TEXT         NOT NULL,
    client_order_id                 TEXT         NOT NULL,
    price                           TEXT         NOT NULL,
    is_bid                          BOOLEAN      NOT NULL,
    base_asset_quantity_canceled    TEXT         NOT NULL,
    onchain_timestamp               TEXT         NOT NULL
);
