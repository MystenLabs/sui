DROP PROCEDURE IF EXISTS advance_partition;
CREATE PROCEDURE advance_partition(
    IN table_name TEXT,
    IN last_epoch BIGINT,
    IN new_epoch BIGINT,
    IN last_epoch_start_cp BIGINT,
    IN new_epoch_start_cp BIGINT
)
BEGIN
SET @sql = CONCAT('ALTER TABLE ', table_name, ' REMOVE PARTITIONING');
PREPARE stmt FROM @sql;
EXECUTE stmt;
DEALLOCATE PREPARE stmt;

SET @sql = CONCAT('ALTER TABLE ', table_name, ' ADD PARTITION (PARTITION ', table_name, '_partition_', new_epoch, ' VALUES LESS THAN (', new_epoch_start_cp, '))');
PREPARE stmt FROM @sql;
EXECUTE stmt;
DEALLOCATE PREPARE stmt;

SET @sql = CONCAT('CREATE TABLE IF NOT EXISTS ', table_name, '_partition_', new_epoch, ' PARTITION OF ', table_name, ' VALUES LESS THAN (MAXVALUE)');
PREPARE stmt FROM @sql;
EXECUTE stmt;
DEALLOCATE PREPARE stmt;
END;