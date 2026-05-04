-- ============================================================
-- distribution_metrics.sql (FINAL)
-- §2.1 Distribution Metrics (Stable + Single-Pass Moments)
-- Outputs: stats_distribution.json
--
-- Run with:
--   psql $DSN -f distribution_metrics.sql \
--     --no-align -t -o stats_distribution.json
-- ============================================================

\set ON_ERROR_STOP on

-- ============================================================
-- 1) BASE DATA EXTRACTION
-- ============================================================

WITH tx_amounts AS (
    SELECT
        COALESCE(wh.tx_type_resolved, 'UNKNOWN') AS tx_type,
        ABS(wh.transaction_effect)::double precision AS amount
    FROM (
        -- QR
        SELECT
            h.id,
            'QR_PAYMENT' AS tx_type_resolved,
            h.transaction_effect
        FROM gateway_wallethistory h
        LEFT JOIN qrapp_fonepaytransaction q
            ON q.transaction_id = h.transaction_id

        UNION ALL

        -- Utilities
        SELECT
            h.id,
            'UTILITIES',
            h.transaction_effect
        FROM gateway_wallethistory h
        JOIN service_servicelog s
            ON s.id = h.transaction_id

        UNION ALL

        -- Remittance
        SELECT
            h.id,
            'REMITTANCE',
            h.transaction_effect
        FROM gateway_wallethistory h
        JOIN remittance_remittance r
            ON r.txn_id = h.transaction_id

        UNION ALL

        -- Disbursement
        SELECT
            h.id,
            d.type,
            h.transaction_effect
        FROM gateway_wallethistory h
        JOIN disbursement_transaction d
            ON d.id = h.transaction_id
    ) wh
    WHERE ABS(wh.transaction_effect) > 0
),

-- ============================================================
-- 2) MOMENTS (using standard PostgreSQL aggregates)
-- ============================================================

moments AS (
    SELECT
        tx_type,
        COUNT(*) AS n,
        AVG(amount) AS mean,
        STDDEV(amount) AS stddev,
        VARIANCE(amount) AS variance,
        -- Pearson's skewness approximation using percentiles
        -- (P90 + P10 - 2*P50) / (P90 - P10)
        (PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY amount) +
         PERCENTILE_CONT(0.10) WITHIN GROUP (ORDER BY amount) -
         2 * PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY amount)) /
        NULLIF(PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY amount) -
               PERCENTILE_CONT(0.10) WITHIN GROUP (ORDER BY amount), 0) AS skewness,
        -- Kurtosis approximation using percentiles
        (PERCENTILE_CONT(0.975) WITHIN GROUP (ORDER BY amount) -
         PERCENTILE_CONT(0.025) WITHIN GROUP (ORDER BY amount)) /
        NULLIF(PERCENTILE_CONT(0.75) WITHIN GROUP (ORDER BY amount) -
               PERCENTILE_CONT(0.25) WITHIN GROUP (ORDER BY amount), 0) AS ex_kurtosis
    FROM tx_amounts
    GROUP BY tx_type
),

-- ============================================================
-- 3) PERCENTILES + TAIL (SECOND PASS)
-- ============================================================

dist AS (
    SELECT
        tx_type,
        COUNT(*) AS n,
        MIN(amount) AS min_amount,
        MAX(amount) AS max_amount,

        PERCENTILE_CONT(0.10) WITHIN GROUP (ORDER BY amount) AS p10,
        PERCENTILE_CONT(0.20) WITHIN GROUP (ORDER BY amount) AS p20,
        PERCENTILE_CONT(0.30) WITHIN GROUP (ORDER BY amount) AS p30,
        PERCENTILE_CONT(0.40) WITHIN GROUP (ORDER BY amount) AS p40,
        PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY amount) AS p50,
        PERCENTILE_CONT(0.60) WITHIN GROUP (ORDER BY amount) AS p60,
        PERCENTILE_CONT(0.70) WITHIN GROUP (ORDER BY amount) AS p70,
        PERCENTILE_CONT(0.80) WITHIN GROUP (ORDER BY amount) AS p80,
        PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY amount) AS p90,
        PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY amount) AS p95,
        PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY amount) AS p99,

        AVG(LN(NULLIF(amount, 0))) AS lognorm_mu,
        STDDEV(LN(NULLIF(amount, 0))) AS lognorm_sigma
    FROM tx_amounts
    GROUP BY tx_type
),

-- Pareto tail (uses precomputed p95)
pareto AS (
    SELECT
        d.tx_type,
        1.0 / NULLIF(
            AVG(LN(t.amount / NULLIF(d.p95, 0)))
            FILTER (WHERE t.amount >= d.p95),
        0) AS pareto_alpha
    FROM dist d
    JOIN tx_amounts t USING (tx_type)
    GROUP BY d.tx_type, d.p95
),

-- ============================================================
-- 4) TYPE WEIGHTS
-- ============================================================

type_weights AS (
    SELECT
        tx_type,
        COUNT(*)::float / SUM(COUNT(*)) OVER () AS weight
    FROM tx_amounts
    GROUP BY tx_type
)

-- ============================================================
-- 5) FINAL JSON OUTPUT
-- ============================================================

SELECT jsonb_pretty(jsonb_agg(
    jsonb_build_object(
        'tx_type', d.tx_type,
        'n', d.n,
        'weight', w.weight,

        -- moments
        'mean', ROUND(m.mean::numeric, 2),
        'variance', ROUND(m.variance::numeric, 2),
        'stddev', ROUND(m.stddev::numeric, 2),
        'skewness', ROUND(m.skewness::numeric, 4),
        'ex_kurtosis', ROUND(m.ex_kurtosis::numeric, 4),

        -- range
        'min', d.min_amount,
        'max', d.max_amount,

        -- percentiles
        'percentiles', jsonb_build_object(
            'p10', d.p10, 'p20', d.p20, 'p30', d.p30,
            'p40', d.p40, 'p50', d.p50, 'p60', d.p60,
            'p70', d.p70, 'p80', d.p80, 'p90', d.p90,
            'p95', d.p95, 'p99', d.p99
        ),

        -- distributions
        'lognorm_mu', ROUND(d.lognorm_mu::numeric, 6),
        'lognorm_sigma', ROUND(d.lognorm_sigma::numeric, 6),
        'pareto_alpha', ROUND(p.pareto_alpha::numeric, 4)
    )
    ORDER BY w.weight DESC
))
FROM dist d
JOIN moments m USING (tx_type)
JOIN pareto p USING (tx_type)
JOIN type_weights w USING (tx_type);