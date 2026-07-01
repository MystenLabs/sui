WITH dense AS (
  SELECT 'dense_everywhere' tier, f.value[0]::string v, f.value[1]::int cnt
  FROM (SELECT APPROX_TOP_K(OBJECT_ID, 10, 100000) topk
        FROM CHAINDATA_MAINNET.TRANSACTION_OBJECT
        WHERE CHECKPOINT < 293000000 AND OBJECT_STATUS IN ('Mutated','Created','Deleted','Wrapped','Unwrapped')),
       LATERAL FLATTEN(input => topk) f
),
recent_window AS (
  SELECT 'recent_only' tier, OBJECT_ID v, COUNT(*) cnt
  FROM CHAINDATA_MAINNET.TRANSACTION_OBJECT
  WHERE CHECKPOINT BETWEEN 287000000 AND 293000000 AND OBJECT_STATUS IN ('Mutated','Created','Deleted','Wrapped','Unwrapped')
  GROUP BY OBJECT_ID HAVING COUNT(*) > 200 ORDER BY COUNT(*) DESC LIMIT 3
),
sparse AS (
  SELECT 'sparse' tier, OBJECT_ID v, COUNT(*) cnt
  FROM CHAINDATA_MAINNET.TRANSACTION_OBJECT
  WHERE CHECKPOINT BETWEEN 292000000 AND 293000000 AND OBJECT_STATUS IN ('Mutated','Created','Deleted','Wrapped','Unwrapped')
  GROUP BY OBJECT_ID HAVING COUNT(*) BETWEEN 20 AND 200 LIMIT 3
)
SELECT tier, v, cnt FROM dense
UNION ALL SELECT tier, v, cnt FROM recent_window
UNION ALL SELECT tier, v, cnt FROM sparse;
