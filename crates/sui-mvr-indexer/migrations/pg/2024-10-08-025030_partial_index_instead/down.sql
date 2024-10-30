-- Drop the new partial indices
DROP INDEX IF EXISTS objects_history_owner_partial;
DROP INDEX IF EXISTS objects_history_coin_owner_partial;
DROP INDEX IF EXISTS objects_history_coin_only_partial;
DROP INDEX IF EXISTS objects_history_type_partial;
DROP INDEX IF EXISTS objects_history_package_module_name_full_type_partial;
DROP INDEX IF EXISTS objects_history_owner_package_module_name_full_type_partial;
