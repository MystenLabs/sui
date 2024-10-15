-- Create new partial indices with object_status = 0 condition
CREATE INDEX IF NOT EXISTS objects_history_owner_partial ON objects_history (checkpoint_sequence_number, owner_type, owner_id) 
WHERE owner_type BETWEEN 1 AND 2 AND owner_id IS NOT NULL AND object_status = 0;

CREATE INDEX IF NOT EXISTS objects_history_coin_owner_partial ON objects_history (checkpoint_sequence_number, owner_id, coin_type, object_id) 
WHERE coin_type IS NOT NULL AND owner_type = 1 AND object_status = 0;

CREATE INDEX IF NOT EXISTS objects_history_coin_only_partial ON objects_history (checkpoint_sequence_number, coin_type, object_id) 
WHERE coin_type IS NOT NULL AND object_status = 0;

CREATE INDEX IF NOT EXISTS objects_history_type_partial ON objects_history (checkpoint_sequence_number, object_type) 
WHERE object_status = 0;

CREATE INDEX IF NOT EXISTS objects_history_package_module_name_full_type_partial ON objects_history (checkpoint_sequence_number, object_type_package, object_type_module, object_type_name, object_type) 
WHERE object_status = 0;

CREATE INDEX IF NOT EXISTS objects_history_owner_package_module_name_full_type_partial ON objects_history (checkpoint_sequence_number, owner_id, object_type_package, object_type_module, object_type_name, object_type) 
WHERE object_status = 0;
