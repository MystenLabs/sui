-- Bloom filter membership check: returns true if all specified bits are set.
-- Returns false as soon as any bit is not set.
--
-- Supports folded bloom filters: byte positions are automatically wrapped
-- using modulo with the actual bloom filter length.
--
-- Parameters:
--   bloom: The bloom filter bytes
--   byte_positions: Array of byte indices to check (will be wrapped to bloom size)
--   bit_masks: Array of bit masks to check (parallel to byte_positions)
--
-- Returns true if all bits are set, false otherwise.
CREATE OR REPLACE FUNCTION bloom_contains(
    bloom bytea,
    byte_positions int[],
    bit_masks int[]
) RETURNS boolean AS $$
DECLARE
    bloom_len int := length(bloom);
BEGIN
    FOR i IN 1..array_length(byte_positions, 1) LOOP
        IF (get_byte(bloom, byte_positions[i] % bloom_len) & bit_masks[i]) = 0 THEN
            RETURN false;
        END IF;
    END LOOP;
    RETURN true;
END;
$$ LANGUAGE plpgsql IMMUTABLE STRICT PARALLEL SAFE;
