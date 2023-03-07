DROP TABLE IF EXISTS owner;
DROP TABLE IF EXISTS owner_history;

DROP FUNCTION object_owned_at_checkpoint(BIGINT, owner_type, address)
