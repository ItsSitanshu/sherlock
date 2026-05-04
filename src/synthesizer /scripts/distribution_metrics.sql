-- ============================================================
-- distribution_metrics.sql
-- §2.1 Distribution Metrics
-- Outputs: stats_distribution.json
--
-- Run with:
--   psql $DSN -f distribution_metrics.sql \
--     --no-align -t -o stats_distribution.json
-- ============================================================

\set ON_ERROR_STOP on

WITH tx_amounts AS (
    -- gateway_wallethistory is the canonical ledger; join sub-tables for type
    SELECT
        COALESCE(wh.tx_type_resolved, 'UNKNOWN') AS tx_type,
        ABS(wh.transaction_effect)                AS amount,
        wh.is_anomaly
    FROM (
        -- QR / Merchant
        SELECT
            h.id,
            CASE
                WHEN q.on_us  THEN 'QR_PAYMENT'
                WHEN q.id IS NULL THEN NULL
                ELSE 'QR_PAYMENT'
            END AS tx_type_resolved,
            h.transaction_effect,
            FALSE AS is_anomaly           -- anomaly label injected later
        FROM gateway_wallethistory h
        LEFT JOIN qrapp_fonepaytransaction q ON q.transaction_id = h.transaction_id

        UNION ALL

        -- Utilities
        SELECT
            h.id,
            'UTILITIES'          AS tx_type_resolved,
            h.transaction_effect,
            FALSE
        FROM gateway_wallethistory h
        JOIN service_servicelog s ON s.id = h.transaction_id

        UNION ALL

        -- Remittance
        SELECT
            h.id,
            'REMITTANCE'         AS tx_type_resolved,
            h.transaction_effect,
            FALSE
        FROM gateway_wallethistory h
        JOIN remittance_remittance r ON r.txn_id = h.transaction_id

        UNION ALL

        -- Disbursement (bank transfer / topup)
        SELECT
            h.id,
            d.type               AS tx_type_resolved,
            h.transaction_effect,
            FALSE
        FROM gateway_wallethistory h
        JOIN disbursement_transaction d ON d.id = h.transaction_id
    ) wh
    WHERE ABS(wh.transaction_effect) > 0
),

per_type AS (
    SELECT
        tx_type,
        COUNT(*)                                     AS n,
        AVG(amount)                                  AS mean,
        VARIANCE(amount)                             AS variance,
        STDDEV(amount)                               AS stddev,
        -- Skewness:  E[(X-μ)³] / σ³
        AVG(POWER(amount - AVG(amount) OVER (PARTITION BY tx_type), 3))
            / NULLIF(POWER(STDDEV(amount) OVER (PARTITION BY tx_type), 3), 0)
                                                     AS skewness,
        -- Excess kurtosis: E[(X-μ)⁴] / σ⁴  - 3
        AVG(POWER(amount - AVG(amount) OVER (PARTITION BY tx_type), 4))
            / NULLIF(POWER(STDDEV(amount) OVER (PARTITION BY tx_type), 4), 0) - 3
                                                     AS ex_kurtosis,
        MIN(amount)                                  AS min_amount,
        MAX(amount)                                  AS max_amount,
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
        -- Log-normal parameters (MLE): μ_ln = E[ln X],  σ_ln = Std[ln X]
        AVG(LN(NULLIF(amount, 0)))                   AS lognorm_mu,
        STDDEV(LN(NULLIF(amount, 0)))                AS lognorm_sigma,
        -- Pareto tail index estimate (Hill estimator) over top 5%
        -- α ≈ 1 / mean(ln(x/x_min)) for x > x_min
        1.0 / NULLIF(
            AVG(LN(amount::float /
                NULLIF(PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY amount), 0)))
            FILTER (WHERE amount >= PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY amount)),
        0)                                           AS pareto_alpha_tail
    FROM tx_amounts
    GROUP BY tx_type
),

-- Overall type-mix weights
type_counts AS (
    SELECT
        tx_type,
        COUNT(*) AS cnt,
        COUNT(*)::float / SUM(COUNT(*)) OVER () AS weight
    FROM tx_amounts
    GROUP BY tx_type
)

SELECT jsonb_pretty(jsonb_agg(
    jsonb_build_object(
        'tx_type',       p.tx_type,
        'n',             p.n,
        'weight',        tc.weight,
        'mean',          ROUND(p.mean::numeric, 2),
        'variance',      ROUND(p.variance::numeric, 2),
        'stddev',        ROUND(p.stddev::numeric, 2),
        'skewness',      ROUND(p.skewness::numeric, 4),
        'ex_kurtosis',   ROUND(p.ex_kurtosis::numeric, 4),
        'min',           p.min_amount,
        'max',           p.max_amount,
        'percentiles', jsonb_build_object(
            'p10', p.p10, 'p20', p.p20, 'p30', p.p30,
            'p40', p.p40, 'p50', p.p50, 'p60', p.p60,
            'p70', p.p70, 'p80', p.p80, 'p90', p.p90,
            'p95', p.p95, 'p99', p.p99
        ),
        'lognorm_mu',    ROUND(p.lognorm_mu::numeric, 6),
        'lognorm_sigma', ROUND(p.lognorm_sigma::numeric, 6),
        'pareto_alpha',  ROUND(p.pareto_alpha_tail::numeric, 4)
    )
    ORDER BY tc.weight DESC
))
FROM per_type p
JOIN type_counts tc USING (tx_type);