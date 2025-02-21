DROP INDEX IF EXISTS obj_info_owner_object_id_desc;
DROP INDEX IF EXISTS obj_info_pkg_object_id_desc;
DROP INDEX IF EXISTS obj_info_mod_object_id_desc;
DROP INDEX IF EXISTS obj_info_name_object_id_desc;
DROP INDEX IF EXISTS obj_info_inst_object_id_desc;
DROP INDEX IF EXISTS obj_info_owner_pkg_object_id_desc;
DROP INDEX IF EXISTS obj_info_owner_mod_object_id_desc;
DROP INDEX IF EXISTS obj_info_owner_name_object_id_desc;
DROP INDEX IF EXISTS obj_info_owner_inst_object_id_desc;

CREATE INDEX IF NOT EXISTS obj_info_owner
ON obj_info (owner_kind, owner_id, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_pkg
ON obj_info (package, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_mod
ON obj_info (package, module, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_name
ON obj_info (package, module, name, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_inst
ON obj_info (package, module, name, instantiation, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_owner_pkg
ON obj_info (owner_kind, owner_id, package, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_owner_mod
ON obj_info (owner_kind, owner_id, package, module, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_owner_name
ON obj_info (owner_kind, owner_id, package, module, name, cp_sequence_number DESC, object_id);

CREATE INDEX IF NOT EXISTS obj_info_owner_inst
ON obj_info (owner_kind, owner_id, package, module, name, instantiation, cp_sequence_number DESC, object_id);
