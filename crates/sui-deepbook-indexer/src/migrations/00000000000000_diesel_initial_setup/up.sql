CREATE TABLE IF NOT EXISTS progress_store
(
    task_name                   TEXT         PRIMARY KEY,
    checkpoint                  BIGINT       NOT NULL,
    target_checkpoint           BIGINT       NOT NULL,
    timestamp                   BIGINT       NOT NULL
);

CREATE TABLE IF NOT EXISTS deepbook
(
    digest                      TEXT         PRIMARY KEY,
    sender                      TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL
);
CREATE INDEX deepbook_sender ON deepbook (sender);

CREATE TABLE IF NOT EXISTS deep_price
(
    digest                      TEXT         PRIMARY KEY,
    sender                      TEXT         NOT NULL,
    target_pool                 TEXT         NOT NULL,
    reference_pool              TEXT         NOT NULL,
    checkpoint                  BIGINT       NOT NULL,
    timestamp                   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP NOT NULL
);