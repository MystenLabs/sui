DROP TABLE IF EXISTS owner;
DROP TABLE IF EXISTS owner_history;

DROP FUNCTION IF EXISTS object_owned_at_checkpoint(BIGINT, owner_type, address);
DROP FUNCTION IF EXISTS owner_history_func();

DROP TRIGGER IF EXISTS owner ON objects;
DROP FUNCTION IF EXISTS owner_modified_func();
