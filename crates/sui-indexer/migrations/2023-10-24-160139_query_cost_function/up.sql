-- Your SQL goes here
CREATE OR REPLACE FUNCTION query_cost(
    query_in text,
    cost OUT float8
 ) RETURNS float8  AS
$$DECLARE
 p json;
BEGIN
    /* get execution plan in JSON */
    EXECUTE 'EXPLAIN (FORMAT JSON) ' || query_in INTO p;
    /* extract total cost */
    SELECT p->0->'Plan'->>'Total Cost'
        INTO cost;
    RETURN;
END;$$ LANGUAGE plpgsql STRICT;
