-- Create new partial indices with object_status = 0 condition
CREATE INDEX IF NOT EXISTS objects_history_owner_partial ON objects_history (checkpoint_sequence_number, owner_type, owner_id)
WHERE owner_type BETWEEN 1 AND 2 AND owner_id IS NOT NULL AND object_status = 0;
