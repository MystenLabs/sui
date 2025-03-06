-- Ensure extensions are available
CREATE EXTENSION IF NOT EXISTS pg_partman;
CREATE EXTENSION IF NOT EXISTS pg_cron;

-- Function to safely set up partitioning for a table
CREATE OR REPLACE FUNCTION safe_create_parent(
    p_schema text,
    p_table text,
    p_control text,
    p_type text,
    p_interval text,
    p_start_partition text,
    p_premake int
) RETURNS text AS $$
DECLARE
    full_table_name text := p_schema || '.' || p_table;
    result_message text;
BEGIN
    -- Check if the table is already managed by pg_partman
    IF NOT EXISTS (
        SELECT 1 FROM part_config
        WHERE parent_table = full_table_name
    ) THEN
        -- If not managed, set up partitioning
        PERFORM create_parent(
            p_parent_table := full_table_name,
            p_control := p_control,
            p_type := p_type,
            p_interval := p_interval,
            p_start_partition := p_start_partition,
            p_premake := p_premake
        );
        result_message := 'CREATED: Partitioning set up for ' || full_table_name;
    ELSE
        -- Table is already managed
        result_message := 'EXISTS: Table ' || full_table_name || ' is already managed by pg_partman';
    END IF;

    RETURN result_message;
END;
$$ LANGUAGE plpgsql;

-- Safely set up partitioning for all tables
-- Group 1: Tables with 10 Million Checkpoint Partitions
SELECT safe_create_parent('public', 'tx_affected_objects', 'cp_sequence_number', 'range', '10000000', '70000000', 1);
SELECT safe_create_parent('public', 'ev_emit_mod', 'cp_sequence_number', 'range', '10000000', '70000000', 1);
SELECT safe_create_parent('public', 'ev_struct_inst', 'cp_sequence_number', 'range', '10000000', '70000000', 1);
SELECT safe_create_parent('public', 'tx_affected_addresses', 'cp_sequence_number', 'range', '10000000', '70000000', 1);

-- Group 2: Constant Tables with 10 Million Checkpoint Partitions
SELECT safe_create_parent('public', 'tx_balance_changes', 'cp_sequence_number', 'range', '10000000', '70000000', 1);
SELECT safe_create_parent('public', 'tx_digests', 'cp_sequence_number', 'range', '10000000', '70000000', 1);
SELECT safe_create_parent('public', 'tx_kinds', 'cp_sequence_number', 'range', '10000000', '70000000', 1);
SELECT safe_create_parent('public', 'tx_calls', 'cp_sequence_number', 'range', '10000000', '70000000', 1);

-- Group 3: Tables with 10,000 Checkpoint Partitions
SELECT safe_create_parent('public', 'obj_info', 'cp_sequence_number', 'range', '10000', '70000000', 1);
SELECT safe_create_parent('public', 'obj_versions', 'cp_sequence_number', 'range', '10000', '70000000', 1);
SELECT safe_create_parent('public', 'coin_balance_buckets', 'cp_sequence_number', 'range', '10000', '70000000', 1);

-- Schedule maintenance to run hourly (if not already scheduled)
DELETE FROM cron.job WHERE command = 'SELECT run_maintenance()';
SELECT cron.schedule('0 * * * *', 'SELECT run_maintenance()');

-- Clean up the helper function
DROP FUNCTION IF EXISTS safe_create_parent;
