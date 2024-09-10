-- Your SQL goes here

CREATE TABLE IF NOT EXISTS order_updates
(
    id                          SERIAL       PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    package                     TEXT         NOT NULL,
    status                      TEXT         NOT NULL,
    pool_id                     TEXT         NOT NULL,
    order_id                    TEXT         NOT NULL,
    client_order_id             BIGINT       NOT NULL,
    price                       BIGINT       NOT NULL,
    is_bid                      BOOLEAN      NOT NULL,
    original_quantity           BIGINT       NOT NULL,
    quantity                    BIGINT       NOT NULL,
    onchain_timestamp           BIGINT       NOT NULL,
    balance_manager_id          TEXT         NOT NULL,
    trader                      TEXT         NOT NULL
);

CREATE TABLE IF NOT EXISTS order_fills
(
    id                          SERIAL       PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    package                     TEXT         NOT NULL,
    pool_id                     TEXT         NOT NULL,
    maker_order_id              TEXT         NOT NULL,
    taker_order_id              TEXT         NOT NULL,
    maker_client_order_id       BIGINT       NOT NULL,
    taker_client_order_id       BIGINT       NOT NULL,
    price                       BIGINT       NOT NULL,
    taker_fee                   BIGINT       NOT NULL,
    maker_fee                   BIGINT       NOT NULL,
    taker_is_bid                BOOLEAN      NOT NULL,
    base_quantity               BIGINT       NOT NULL,
    quote_quantity              BIGINT       NOT NULL,
    maker_balance_manager_id    TEXT         NOT NULL,
    taker_balance_manager_id    TEXT         NOT NULL,
    onchain_timestamp           BIGINT       NOT NULL
);

CREATE TABLE IF NOT EXISTS flashloans
(
    id                          SERIAL       PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    package                     TEXT         NOT NULL,
    borrow                      BOOLEAN      NOT NULL,
    pool_id                     TEXT         NOT NULL,
    borrow_quantity             BIGINT       NOT NULL,
    type_name                   TEXT         NOT NULL
);

CREATE TABLE IF NOT EXISTS pool_prices
(
    id                          SERIAL       PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    package                     TEXT         NOT NULL,
    target_pool                 TEXT         NOT NULL,
    reference_pool              TEXT         NOT NULL,
    conversion_rate             BIGINT       NOT NULL
);

CREATE TABLE IF NOT EXISTS balances
(
    id                          SERIAL       PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    package                     TEXT         NOT NULL,
    balance_manager_id          TEXT         NOT NULL,
    asset                       TEXT         NOT NULL,
    amount                      BIGINT       NOT NULL,
    deposit                     BOOLEAN      NOT NULL
);

CREATE TABLE IF NOT EXISTS trade_params_update
(
    id                          SERIAL       PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    package                     TEXT         NOT NULL,
    pool_id                     TEXT         NOT NULL,
    taker_fee                   BIGINT       NOT NULL,
    maker_fee                   BIGINT       NOT NULL,
    stake_required              BIGINT       NOT NULL
);

CREATE TABLE IF NOT EXISTS stakes
(
    id                          SERIAL       PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    package                     TEXT         NOT NULL,
    pool_id                     TEXT         NOT NULL,
    balance_manager_id          TEXT         NOT NULL,
    epoch                       BIGINT       NOT NULL,
    amount                      BIGINT       NOT NULL,
    stake                       BOOLEAN      NOT NULL
);

CREATE TABLE IF NOT EXISTS proposals
(
    id                          SERIAL       PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    package                     TEXT         NOT NULL,
    pool_id                     TEXT         NOT NULL,
    balance_manager_id          TEXT         NOT NULL,
    epoch                       BIGINT       NOT NULL,
    taker_fee                   BIGINT       NOT NULL,
    maker_fee                   BIGINT       NOT NULL,
    stake_required              BIGINT       NOT NULL
);

CREATE TABLE IF NOT EXISTS votes
(
    id                          SERIAL       PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    package                     TEXT         NOT NULL,
    pool_id                     TEXT         NOT NULL,
    balance_manager_id          TEXT         NOT NULL,
    epoch                       BIGINT       NOT NULL,
    from_proposal_id            TEXT,
    to_proposal_id              TEXT         NOT NULL,
    stake                       BIGINT       NOT NULL
);

CREATE TABLE IF NOT EXISTS rebates
(
    id                          SERIAL       PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    package                     TEXT         NOT NULL,
    pool_id                     TEXT         NOT NULL,
    balance_manager_id          TEXT         NOT NULL,
    epoch                       BIGINT       NOT NULL,
    claim_amount                BIGINT       NOT NULL
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
    id                          SERIAL       PRIMARY KEY,
    txn_digest                  TEXT         NOT NULL,
    sender_address              TEXT         NOT NULL,
    timestamp_ms                BIGINT       NOT NULL,
    failure_status              TEXT         NOT NULL,
    package                     TEXT         NOT NULL,
    cmd_idx                     BIGINT
);