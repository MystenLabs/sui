CREATE OR REPLACE PROCEDURE robust_advance_partition(
    table_name TEXT,
    last_epoch BIGINT,
    new_epoch BIGINT,
    new_epoch_start BIGINT
)
LANGUAGE plpgsql
AS $$
DECLARE
    partdef TEXT;
    from_val TEXT;
    to_val   TEXT;
    partition_name TEXT := format('%I_partition_%s', table_name, last_epoch);
BEGIN
    SELECT pg_get_expr(c.relpartbound, c.oid)
      INTO partdef
      FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
     WHERE c.relname = partition_name
       AND c.relispartition
     LIMIT 1;
    SELECT substring(partdef from 'FROM \(([^\)]+)\)'),
           substring(partdef from 'TO \(([^\)]+)\)')
      INTO from_val, to_val;

    EXECUTE format('ALTER TABLE %I DETACH PARTITION %I', table_name, partition_name);
    EXECUTE format(
        'ALTER TABLE %I ATTACH PARTITION %I FOR VALUES FROM (%s) TO (%L)',
        table_name,
        partition_name,
        from_val,
        new_epoch_start
    );
    EXECUTE format(
        'CREATE TABLE IF NOT EXISTS %I_partition_%s PARTITION OF %I '
        || 'FOR VALUES FROM (%L) TO (MAXVALUE)',
        table_name,
        new_epoch,
        table_name,
        new_epoch_start
    );
END;
$$;
