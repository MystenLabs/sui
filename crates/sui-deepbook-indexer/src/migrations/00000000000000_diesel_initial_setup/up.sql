-- Your SQL goes here



CREATE TABLE IF NOT EXISTS order_updates
(
    digest                      TEXT         PRIMARY KEY,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    status                      TEXT         NOT NULL,
    pool_id                     TEXT         NOT NULL,
    order_id                    NUMERIC      NOT NULL,
    client_order_id             BIGINT       NOT NULL,
    price                       BIGINT       NOT NULL,
    is_bid                      BOOLEAN      NOT NULL,
    quantity                    BIGINT       NOT NULL,
    onchain_timestamp           BIGINT       NOT NULL,
    balance_manager_id          TEXT         NOT NULL,
    trader                      TEXT         NOT NULL
);

CREATE TABLE IF NOT EXISTS order_fills
(
    digest                      TEXT         PRIMARY KEY,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    pool_id                     TEXT         NOT NULL,
    maker_order_id              NUMERIC      NOT NULL,
    taker_order_id              NUMERIC      NOT NULL,
    maker_client_order_id       BIGINT       NOT NULL,
    taker_client_order_id       BIGINT       NOT NULL,
    price                       BIGINT       NOT NULL,
    taker_is_bid                BOOLEAN      NOT NULL,
    base_quantity               BIGINT       NOT NULL,
    quote_quantity              BIGINT       NOT NULL,
    maker_balance_manager_id    TEXT         NOT NULL,
    taker_balance_manager_id    TEXT         NOT NULL,
    onchain_timestamp           BIGINT       NOT NULL
);

CREATE TABLE IF NOT EXISTS flashloans
(
    digest                      TEXT         PRIMARY KEY,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    borrow                      BOOLEAN      NOT NULL,
    pool_id                     TEXT         NOT NULL,
    borrow_quantity             BIGINT       NOT NULL,
    type_name                   TEXT         NOT NULL
);

CREATE TABLE IF NOT EXISTS pool_prices
(
    digest                      TEXT         PRIMARY KEY,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    target_pool                 TEXT         NOT NULL,
    reference_pool              TEXT         NOT NULL,
    conversion_rate             BIGINT       NOT NULL
);

CREATE TABLE IF NOT EXISTS progress_store
(
    task_name                   TEXT          PRIMARY KEY,
    checkpoint                  BIGINT        NOT NULL,
    target_checkpoint           BIGINT        DEFAULT 9223372036854775807 NOT NULL,
    timestamp                   TIMESTAMP     DEFAULT now()
);

CREATE TABLE IF NOT EXISTS sui_error_transactions
(
    txn_digest                  TEXT         PRIMARY KEY,
    sender_address              TEXT         NOT NULL,
    timestamp_ms                BIGINT       NOT NULL,
    failure_status              TEXT         NOT NULL,
    cmd_idx                     BIGINT
);