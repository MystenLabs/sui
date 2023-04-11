CREATE MATERIALIZED VIEW epoch_network_metrics as
SELECT MAX(tps_30_days) as tps_30_days
FROM (SELECT (((SUM(total_transactions) OVER w) - (FIRST_VALUE(total_transactions) OVER w))::float8 /
              ((MAX(timestamp_ms) OVER w - MIN(timestamp_ms) OVER w)) *
              1000) AS tps_30_days
      FROM checkpoints
      WHERE timestamp_ms / 1000 > (EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - '30 days'::INTERVAL)))::BIGINT
      WINDOW w AS (ORDER BY timestamp_ms ROWS BETWEEN 14 PRECEDING AND 15 FOLLOWING)) t1;

CREATE TABLE epochs
(
    epoch                           BIGINT PRIMARY KEY,
    first_checkpoint_id             BIGINT   NOT NULL,
    last_checkpoint_id              BIGINT,
    epoch_start_timestamp           BIGINT   NOT NULL,
    epoch_end_timestamp             BIGINT,
    epoch_total_transactions        BIGINT   NOT NULL,

    -- end of epoch data
    next_epoch_version              BIGINT,
    next_epoch_committee            bytea[]  NOT NULL,
    next_epoch_committee_stake      BIGINT[] NOT NULL,
    epoch_commitments               bytea[]  NOT NULL,

    protocol_version                BIGINT,
    reference_gas_price             BIGINT,
    total_stake                     BIGINT,
    storage_fund_reinvestment       BIGINT,
    storage_charge                  BIGINT,
    storage_rebate                  BIGINT,
    storage_fund_balance            BIGINT,
    stake_subsidy_amount            BIGINT,
    total_gas_fees                  BIGINT,
    total_stake_rewards_distributed BIGINT,
    leftover_storage_fund_inflow    BIGINT
);
CREATE INDEX epochs_start_index ON epochs (epoch_start_timestamp ASC);
CREATE INDEX epochs_end_index ON epochs (epoch_end_timestamp ASC NULLS LAST);

-- update epoch_network_metrics on every epoch
CREATE OR REPLACE FUNCTION refresh_view_func() RETURNS TRIGGER AS
$body$
BEGIN
    IF (TG_OP = 'INSERT') THEN
        REFRESH MATERIALIZED VIEW epoch_network_metrics;
        REFRESH MATERIALIZED VIEW epoch_move_call_metrics;
        RETURN NEW;
    ELSEIF (TG_OP = 'UPDATE') THEN
        RETURN NEW;
    ELSIF (TG_OP = 'DELETE') THEN
        RETURN OLD;
    ELSE
        RAISE WARNING '[REFRESH_VIEW_FUN] - Other action occurred: %, at %',TG_OP,NOW();
        RETURN NULL;
    END IF;

EXCEPTION
    WHEN data_exception THEN
        RAISE WARNING '[REFRESH_VIEW_FUN] - UDF ERROR [DATA EXCEPTION] - SQLSTATE: %, SQLERRM: %',SQLSTATE,SQLERRM;
        RETURN NULL;
    WHEN unique_violation THEN
        RAISE WARNING '[REFRESH_VIEW_FUN] - UDF ERROR [UNIQUE] - SQLSTATE: %, SQLERRM: %',SQLSTATE,SQLERRM;
        RETURN NULL;
    WHEN OTHERS THEN
        RAISE WARNING '[REFRESH_VIEW_FUN] - UDF ERROR [OTHER] - SQLSTATE: %, SQLERRM: %',SQLSTATE,SQLERRM;
        RETURN NULL;
END;
$body$
    LANGUAGE plpgsql;

CREATE TRIGGER refresh_view
    AFTER INSERT
    ON epochs
    FOR EACH ROW
EXECUTE PROCEDURE refresh_view_func();

CREATE MATERIALIZED VIEW epoch_move_call_metrics AS
(SELECT 3::BIGINT AS day, move_package, move_module, move_function, COUNT(*) AS count
 FROM move_calls
 WHERE epoch >
       (SELECT MIN(epoch)
        FROM epochs
        WHERE epoch_start_timestamp > ((EXTRACT(EPOCH FROM CURRENT_TIMESTAMP - '3 days'::INTERVAL)) * 1000)::BIGINT)
 GROUP BY move_package, move_module, move_function
 LIMIT 10)
UNION ALL
(SELECT 7::BIGINT AS day, move_package, move_module, move_function, COUNT(*) AS count
 FROM move_calls
 WHERE epoch >
       (SELECT MIN(epoch)
        FROM epochs
        WHERE epoch_start_timestamp > ((EXTRACT(EPOCH FROM CURRENT_TIMESTAMP - '7 days'::INTERVAL)) * 1000)::BIGINT)
 GROUP BY move_package, move_module, move_function
 LIMIT 10)
UNION ALL
(SELECT 30::BIGINT AS day, move_package, move_module, move_function, COUNT(*) AS count
 FROM move_calls
 WHERE epoch >
       (SELECT MIN(epoch)
        FROM epochs
        WHERE epoch_start_timestamp > ((EXTRACT(EPOCH FROM CURRENT_TIMESTAMP - '30 days'::INTERVAL)) * 1000)::BIGINT)
 GROUP BY move_package, move_module, move_function
 LIMIT 10);
