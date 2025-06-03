-- Add the new non-null fields with default values
ALTER TABLE obj_info
ADD COLUMN marked_obsolete BOOLEAN NOT NULL DEFAULT FALSE,
ADD COLUMN marked_predecessor BOOLEAN NOT NULL DEFAULT FALSE;

-- Partial indexes - only index TRUE values for efficiency
CREATE INDEX IF NOT EXISTS obj_info_marked_obsolete ON obj_info (object_id, cp_sequence_number) WHERE marked_obsolete = TRUE;
CREATE INDEX IF NOT EXISTS obj_info_marked_predecessor ON obj_info (object_id, cp_sequence_number) WHERE marked_predecessor = TRUE;

-- Composite index for common query patterns
CREATE INDEX IF NOT EXISTS obj_info_can_delete ON obj_info (object_id, cp_sequence_number) WHERE marked_obsolete = TRUE AND marked_predecessor = TRUE;

-- If you need to query by checkpoint range with these flags
CREATE INDEX IF NOT EXISTS obj_info_obsolete_by_cp ON obj_info (cp_sequence_number DESC, object_id) WHERE marked_obsolete = TRUE;
CREATE INDEX IF NOT EXISTS obj_info_predecessor_by_cp ON obj_info (cp_sequence_number DESC, object_id) WHERE marked_predecessor = TRUE;
