CREATE OR REPLACE PROCEDURE advance_partition(table_name TEXT, last_epoch BIGINT, new_epoch BIGINT, last_epoch_start BIGINT, new_epoch_start BIGINT)
LANGUAGE plpgsql
AS $$
BEGIN
    EXECUTE format('ALTER TABLE %I DETACH PARTITION %I_partition_%s', table_name, table_name, last_epoch);
    EXECUTE format('ALTER TABLE %I ATTACH PARTITION %I_partition_%s FOR VALUES FROM (%L) TO (%L)', table_name, table_name, last_epoch, last_epoch_start, new_epoch_start);
    EXECUTE format('CREATE TABLE IF NOT EXISTS %I_partition_%s PARTITION OF %I FOR VALUES FROM (%L) TO (MAXVALUE)', table_name, new_epoch, table_name, new_epoch_start);
END;
$$;

CREATE OR REPLACE PROCEDURE drop_partition(table_name TEXT, epoch BIGINT)
LANGUAGE plpgsql
AS $$
BEGIN
    EXECUTE format('DROP TABLE IF EXISTS %I_partition_%s', table_name, epoch);
END;
$$;
