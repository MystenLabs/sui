-- Your SQL goes here
DROP FUNCTION IF EXISTS query_cost;
CREATE FUNCTION query_cost(query_in TEXT)
RETURNS FLOAT
DETERMINISTIC
BEGIN
    DECLARE cost FLOAT;
    SET cost = 1.0;
    RETURN cost;
END;