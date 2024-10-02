ALTER TABLE objects
ADD COLUMN df_name bytea,
ADD COLUMN df_object_type text,
ADD COLUMN df_object_id bytea,
ADD COLUMN checkpoint_sequence_number bigint;

ALTER TABLE objects_snapshot
ADD COLUMN df_name bytea,
ADD COLUMN df_object_type text,
ADD COLUMN df_object_id bytea;

ALTER TABLE objects_history
ADD COLUMN df_name bytea,
ADD COLUMN df_object_type text,
ADD COLUMN df_object_id bytea;
