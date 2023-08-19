CREATE TABLE epochs
(
    epoch                           BIGINT      PRIMARY KEY,
    validators                      bytea[]     NOT NULL,
    epoch_total_transactions        BIGINT      NOT NULL,
    first_checkpoint_id             BIGINT      NOT NULL,
    epoch_start_timestamp           BIGINT      NOT NULL,
    reference_gas_price             BIGINT      NOT NULL,
    protocol_version                BIGINT      NOT NULL,
    end_of_epoch_info               bytea
);