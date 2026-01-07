-- Function to perform bitwise OR on two bytea values.
-- Used for merging bloom filters when upserting partial blocks.
CREATE OR REPLACE FUNCTION bytea_or(a bytea, b bytea)
RETURNS bytea AS $$
DECLARE
    result bytea;
    i integer;
BEGIN
    IF a IS NULL THEN RETURN b; END IF;
    IF b IS NULL THEN RETURN a; END IF;

    IF length(a) <> length(b) THEN
        RAISE EXCEPTION 'bytea_or: arguments must have equal length (% vs %)', length(a), length(b);
    END IF;

    result := a;
    FOR i IN 0..length(a)-1 LOOP
        result := set_byte(result, i, get_byte(a, i) | get_byte(b, i));
    END LOOP;

    RETURN result;
END;
$$ LANGUAGE plpgsql IMMUTABLE STRICT PARALLEL SAFE;
