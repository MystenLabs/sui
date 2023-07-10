CREATE MATERIALIZED VIEW epoch_network_metrics as
WITH checkpoints_30d AS (
  SELECT
    MAX(sequence_number) AS sequence_number,
    SUM(total_successful_transactions) AS total_successful_transactions,
    timestamp_ms
  FROM
    checkpoints
  WHERE
    timestamp_ms > EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - INTERVAL '30 days')) * 1000
  GROUP BY
    timestamp_ms
),
tps_data AS (
  SELECT
    sequence_number,
    total_successful_transactions,
    LAG(timestamp_ms) OVER (ORDER BY timestamp_ms DESC) - timestamp_ms AS time_diff
  FROM 
    checkpoints_30d
)
SELECT 
  MAX(total_successful_transactions * 1000.0 / time_diff)::float8 as tps_30_days
FROM 
  tps_data
WHERE 
  time_diff IS NOT NULL;

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
DECLARE
    attempts INT := 0;
BEGIN
    IF (TG_OP = 'INSERT') THEN
        LOOP
            BEGIN
                attempts := attempts + 1;
                REFRESH MATERIALIZED VIEW epoch_network_metrics;
                REFRESH MATERIALIZED VIEW epoch_move_call_metrics;
                EXIT;
            EXCEPTION
                WHEN OTHERS THEN
                    IF attempts >= 10 THEN
                        RAISE WARNING '[REFRESH_VIEW_FUN] - UDF ERROR [OTHER] - SQLSTATE: %, SQLERRM: %', SQLSTATE, SQLERRM;
                        RETURN NULL;
                    END IF;
                    RAISE WARNING '[REFRESH_VIEW_FUN] - Retry failed, attempting again in 1 second';
                    PERFORM pg_sleep(1);
            END;
        END LOOP;
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
 ORDER BY count DESC
 LIMIT 10)
UNION ALL
(SELECT 7::BIGINT AS day, move_package, move_module, move_function, COUNT(*) AS count
 FROM move_calls
 WHERE epoch >
       (SELECT MIN(epoch)
        FROM epochs
        WHERE epoch_start_timestamp > ((EXTRACT(EPOCH FROM CURRENT_TIMESTAMP - '7 days'::INTERVAL)) * 1000)::BIGINT)
 GROUP BY move_package, move_module, move_function
 ORDER BY count DESC
 LIMIT 10)
UNION ALL
(SELECT 30::BIGINT AS day, move_package, move_module, move_function, COUNT(*) AS count
 FROM move_calls
 WHERE epoch >
       (SELECT MIN(epoch)
        FROM epochs
        WHERE epoch_start_timestamp > ((EXTRACT(EPOCH FROM CURRENT_TIMESTAMP - '30 days'::INTERVAL)) * 1000)::BIGINT)
 GROUP BY move_package, move_module, move_function
 ORDER BY count DESC
 LIMIT 10);
