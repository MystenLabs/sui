WITH agg AS (
  SELECT EVENT_TYPE v, COUNT(*) cnt, MIN(CHECKPOINT) lo, MAX(CHECKPOINT) hi
  FROM CHAINDATA_MAINNET.EVENT
  WHERE CHECKPOINT < 298000000 AND EVENT_TYPE IS NOT NULL
  GROUP BY 1
),
tiered AS (
  SELECT v, cnt, lo, hi,
    CASE
      WHEN lo < 5000000 AND hi >= 293000000 THEN 'dense_everywhere'
      WHEN lo >= 292000000                                      THEN 'recent_only'
      WHEN cnt > 5000 AND (hi - lo) < 500000                      THEN 'bursty'
      WHEN cnt BETWEEN 20 AND 500                                 THEN 'sparse'
      ELSE 'other'
    END tier
  FROM agg
)
SELECT tier, v, cnt, lo, hi
FROM (SELECT tiered.*, ROW_NUMBER() OVER (PARTITION BY tier ORDER BY cnt DESC) rn FROM tiered)
WHERE tier <> 'other' AND rn <= 3
ORDER BY tier, cnt DESC;
