-- Add the new non-null fields with default values
ALTER TABLE obj_info
ADD COLUMN obsolete_at BIGINT,
ADD COLUMN marked_predecessor BOOLEAN NOT NULL DEFAULT FALSE;

-- 1. PREDECESSOR LOOKUP (T0 critical path)
CREATE INDEX obj_info_predecessor_lookup ON obj_info (object_id, cp_sequence_number DESC);

-- 2. DUAL-FLAGGED DELETION (T2 Part 1)
CREATE INDEX obj_info_can_delete ON obj_info (cp_sequence_number, object_id)
WHERE obsolete_at IS NOT NULL AND marked_predecessor = TRUE;

-- 3. CROSS-CHECKPOINT CLEANUP (T2 Part 2)
CREATE INDEX obj_info_obsoleted_by_range ON obj_info (obsolete_at, object_id)
WHERE obsolete_at IS NOT NULL AND marked_predecessor = TRUE;

-- 4. CHECKPOINT RANGE PROCESSING (T0, T1, T2)
CREATE INDEX obj_info_by_checkpoint ON obj_info (cp_sequence_number, object_id);
