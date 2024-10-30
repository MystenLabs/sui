ALTER TABLE objects
DROP COLUMN df_name,
DROP COLUMN df_object_type,
DROP COLUMN df_object_id,
DROP COLUMN checkpoint_sequence_number;

ALTER TABLE objects_snapshot
DROP COLUMN df_name,
DROP COLUMN df_object_type,
DROP COLUMN df_object_id;

ALTER TABLE objects_history
DROP COLUMN df_name,
DROP COLUMN df_object_type,
DROP COLUMN df_object_id;
