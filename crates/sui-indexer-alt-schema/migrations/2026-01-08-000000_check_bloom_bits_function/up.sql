-- Function to check if all specified bits are set in a bloom filter.
-- Returns false as soon as any bit is not set.
-- This is critical for bloom filter performance where most rows are rejected.
--
-- Parameters:
--   bloom: The bloom filter bytes
--   byte_positions: Array of byte indices to check
--   bit_masks: Array of bit masks (one per byte position)
--
-- Returns true if all bits are set, false otherwise.
CREATE OR REPLACE FUNCTION check_bloom_bits(
    bloom bytea,
    byte_positions int[],
    bit_masks int[]
) RETURNS boolean AS $$
BEGIN
    FOR i IN 1..array_length(byte_positions, 1) LOOP
        IF (get_byte(bloom, byte_positions[i]) & bit_masks[i]) = 0 THEN
            RETURN false;
        END IF;
    END LOOP;
    RETURN true;
END;
$$ LANGUAGE plpgsql IMMUTABLE STRICT PARALLEL SAFE;
