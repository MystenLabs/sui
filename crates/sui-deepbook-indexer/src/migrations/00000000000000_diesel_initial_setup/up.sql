-- Your SQL goes here

CREATE TABLE IF NOT EXISTS order_updates
(
    event_digest                TEXT         PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    checkpoint_timestamp_ms     BIGINT       NOT NULL,
    package                     TEXT         NOT NULL,
    status                      TEXT         NOT NULL,
    pool_id                     TEXT         NOT NULL,
    order_id                    TEXT         NOT NULL,
    client_order_id             BIGINT       NOT NULL,
    price                       BIGINT       NOT NULL,
    is_bid                      BOOLEAN      NOT NULL,
    original_quantity           BIGINT       NOT NULL,
    quantity                    BIGINT       NOT NULL,
    filled_quantity             BIGINT       NOT NULL,
    onchain_timestamp           BIGINT       NOT NULL,
    balance_manager_id          TEXT         NOT NULL,
    trader                      TEXT         NOT NULL
);

CREATE TABLE IF NOT EXISTS order_fills
(
    event_digest                TEXT         PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    checkpoint_timestamp_ms     BIGINT       NOT NULL,
    package                     TEXT         NOT NULL,
    pool_id                     TEXT         NOT NULL,
    maker_order_id              TEXT         NOT NULL,
    taker_order_id              TEXT         NOT NULL,
    maker_client_order_id       BIGINT       NOT NULL,
    taker_client_order_id       BIGINT       NOT NULL,
    price                       BIGINT       NOT NULL,
    taker_fee                   BIGINT       NOT NULL,
    taker_fee_is_deep           BOOLEAN      NOT NULL,
    maker_fee                   BIGINT       NOT NULL,
    maker_fee_is_deep           BOOLEAN      NOT NULL,
    taker_is_bid                BOOLEAN      NOT NULL,
    base_quantity               BIGINT       NOT NULL,
    quote_quantity              BIGINT       NOT NULL,
    maker_balance_manager_id    TEXT         NOT NULL,
    taker_balance_manager_id    TEXT         NOT NULL,
    onchain_timestamp           BIGINT       NOT NULL
);

CREATE TABLE IF NOT EXISTS flashloans
(
    event_digest                TEXT         PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    checkpoint_timestamp_ms     BIGINT       NOT NULL,
    package                     TEXT         NOT NULL,
    borrow                      BOOLEAN      NOT NULL,
    pool_id                     TEXT         NOT NULL,
    borrow_quantity             BIGINT       NOT NULL,
    type_name                   TEXT         NOT NULL
);

CREATE TABLE IF NOT EXISTS pool_prices
(
    event_digest                TEXT         PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    checkpoint_timestamp_ms     BIGINT       NOT NULL,
    package                     TEXT         NOT NULL,
    target_pool                 TEXT         NOT NULL,
    reference_pool              TEXT         NOT NULL,
    conversion_rate             BIGINT       NOT NULL
);

CREATE TABLE IF NOT EXISTS balances
(
    event_digest                TEXT         PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    checkpoint_timestamp_ms     BIGINT       NOT NULL,
    package                     TEXT         NOT NULL,
    balance_manager_id          TEXT         NOT NULL,
    asset                       TEXT         NOT NULL,
    amount                      BIGINT       NOT NULL,
    deposit                     BOOLEAN      NOT NULL
);

CREATE TABLE IF NOT EXISTS trade_params_update
(
    event_digest                TEXT         PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    checkpoint_timestamp_ms     BIGINT       NOT NULL,
    package                     TEXT         NOT NULL,
    pool_id                     TEXT         NOT NULL,
    taker_fee                   BIGINT       NOT NULL,
    maker_fee                   BIGINT       NOT NULL,
    stake_required              BIGINT       NOT NULL
);

CREATE TABLE IF NOT EXISTS stakes
(
    event_digest                TEXT         PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    checkpoint_timestamp_ms     BIGINT       NOT NULL,
    package                     TEXT         NOT NULL,
    pool_id                     TEXT         NOT NULL,
    balance_manager_id          TEXT         NOT NULL,
    epoch                       BIGINT       NOT NULL,
    amount                      BIGINT       NOT NULL,
    stake                       BOOLEAN      NOT NULL
);

CREATE TABLE IF NOT EXISTS proposals
(
    event_digest                TEXT         PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    checkpoint_timestamp_ms     BIGINT       NOT NULL,
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
    event_digest                TEXT         PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    checkpoint_timestamp_ms     BIGINT       NOT NULL,
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
    event_digest                TEXT         PRIMARY KEY,
    digest                      TEXT         NOT NULL,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL,
    checkpoint_timestamp_ms     BIGINT       NOT NULL,
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

CREATE TABLE IF NOT EXISTS pools
(
    pool_id                     TEXT         PRIMARY KEY,
    pool_name                   TEXT         NOT NULL,
    base_asset_id               TEXT         NOT NULL,
    base_asset_decimals         SMALLINT     NOT NULL,
    base_asset_symbol           TEXT         NOT NULL,
    base_asset_name             TEXT         NOT NULL,
    quote_asset_id              TEXT         NOT NULL,
    quote_asset_decimals        SMALLINT     NOT NULL,
    quote_asset_symbol          TEXT         NOT NULL,
    quote_asset_name            TEXT         NOT NULL,
    min_size                    INTEGER      NOT NULL,
    lot_size                    INTEGER      NOT NULL,
    tick_size                   INTEGER      NOT NULL
);

CREATE TABLE IF NOT EXISTS assets
(
    type                      TEXT         PRIMARY KEY,
    name                      TEXT         NOT NULL,
    symbol                    TEXT         NOT NULL,
    decimals                  SMALLINT     NOT NULL,
    ucid                      INT,
    package_id                TEXT,
    package_address_url       TEXT
);
