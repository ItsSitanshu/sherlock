-- ============================================================
-- relational_structure.sql
-- §2.3 Relational Structure (Graphs)
-- Outputs: stats_graph.json
--
-- Run with:
--   psql $DSN -f relational_structure.sql \
--     --no-align -t -o stats_graph.json
-- ============================================================

\set ON_ERROR_STOP on

-- ── User-Merchant edge table from QR transactions ────────────────────────────
-- acquiree = merchant, issuee = user (Fonepay convention)
WITH edges AS (
    SELECT
        issuee_id   AS user_id,
        acquiree_id AS merchant_id,
        COUNT(*)    AS tx_count,
        SUM(amount) AS total_amount
    FROM qrapp_fonepaytransaction
    WHERE issuee_id IS NOT NULL
      AND acquiree_id IS NOT NULL
    GROUP BY issuee_id, acquiree_id
),

-- Also include disbursement-based edges (topup / bank transfer destination)
dis_edges AS (
    SELECT
        source_account_id AS user_id,
        destination_account_id AS merchant_id,
        COUNT(*)          AS tx_count,
        SUM(amount)       AS total_amount
    FROM disbursement_transaction
    WHERE source_account_id IS NOT NULL
      AND destination_account_id IS NOT NULL
    GROUP BY source_account_id, destination_account_id
),

all_edges AS (
    SELECT user_id, merchant_id, SUM(tx_count) AS tx_count, SUM(total_amount) AS total_amount
    FROM (SELECT * FROM edges UNION ALL SELECT * FROM dis_edges) e
    GROUP BY user_id, merchant_id
),

-- ── User degree (# unique merchants per user) ────────────────────────────────
user_degree AS (
    SELECT
        user_id,
        COUNT(DISTINCT merchant_id) AS degree,
        SUM(tx_count)               AS total_tx
    FROM all_edges
    GROUP BY user_id
),

user_degree_dist AS (
    SELECT
        AVG(degree)                                              AS mean_degree,
        STDDEV(degree)                                           AS stddev_degree,
        PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY degree)    AS p50_degree,
        PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY degree)    AS p90_degree,
        PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY degree)    AS p99_degree,
        MAX(degree)                                              AS max_degree,
        -- Power-law exponent estimate (MLE for discrete Pareto)
        -- γ = 1 + n * (sum ln(xi / xmin))^-1  where xmin = 1
        1.0 + COUNT(*)::float / NULLIF(SUM(LN(GREATEST(degree, 1))), 0) AS gamma_user_degree
    FROM user_degree
),

-- ── Merchant degree (# unique users per merchant) ────────────────────────────
merchant_degree AS (
    SELECT
        merchant_id,
        COUNT(DISTINCT user_id) AS degree
    FROM all_edges
    GROUP BY merchant_id
),

merchant_degree_dist AS (
    SELECT
        AVG(degree)                                           AS mean_degree,
        STDDEV(degree)                                        AS stddev_degree,
        PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY degree) AS p50,
        PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY degree) AS p90,
        MAX(degree)                                           AS max_degree,
        1.0 + COUNT(*)::float / NULLIF(SUM(LN(GREATEST(degree, 1))), 0) AS gamma_merchant_degree
    FROM merchant_degree
),

-- ── Edge weight distribution (tx count per user-merchant pair) ───────────────
edge_weight_dist AS (
    SELECT
        AVG(tx_count)                                             AS mean_edge_weight,
        STDDEV(tx_count)                                          AS stddev_edge_weight,
        PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY tx_count)   AS p50_weight,
        PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY tx_count)   AS p90_weight,
        PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY tx_count)   AS p99_weight,
        MAX(tx_count)                                             AS max_weight,
        -- Geometric distribution p estimate: p = 1 / mean
        1.0 / NULLIF(AVG(tx_count), 0)                           AS geometric_p,
        -- Fraction of edges with >1 tx (repeat rate)
        COUNT(*) FILTER (WHERE tx_count > 1)::float / COUNT(*)   AS repeat_edge_rate
    FROM all_edges
),

-- ── Unique devices per user ──────────────────────────────────────────────────
-- Uses device_id from qrapp_fonepaytransaction, joined via issuee_id = user
device_per_user AS (
    SELECT
        issuee_id                   AS user_id,
        COUNT(DISTINCT device_id)   AS n_devices
    FROM qrapp_fonepaytransaction
    WHERE issuee_id IS NOT NULL
      AND device_id IS NOT NULL AND device_id <> ''
    GROUP BY issuee_id
),

device_dist AS (
    SELECT
        AVG(n_devices)                                           AS mean_devices,
        STDDEV(n_devices)                                        AS stddev_devices,
        PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY n_devices)  AS p50,
        PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY n_devices)  AS p90,
        PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY n_devices)  AS p99,
        MAX(n_devices)                                           AS max_devices,
        COUNT(*) FILTER (WHERE n_devices > 1)::float / COUNT(*)  AS multi_device_ratio
    FROM device_per_user
),

-- ── Feature covariance (amount, fee, service_charge) ────────────────────────
covariance_features AS (
    SELECT
        CORR(amount::float, fee_amount::float)     AS corr_amount_fee,
        CORR(amount::float, discount_amount::float) AS corr_amount_discount,
        CORR(amount::float, markup_amount::float)  AS corr_amount_markup,
        AVG(amount)                                AS mean_amount,
        AVG(fee_amount)                            AS mean_fee,
        STDDEV(amount)                             AS std_amount,
        STDDEV(fee_amount)                         AS std_fee
    FROM qrapp_fonepaytransaction
    WHERE amount > 0
),

-- ── Repeat transaction frequency per user (not per edge) ────────────────────
user_tx_counts AS (
    SELECT
        user_id,
        COUNT(*) AS tx_count
    FROM gateway_wallethistory
    GROUP BY user_id
),

user_tx_dist AS (
    SELECT
        AVG(tx_count)                                            AS mean_tx_per_user,
        STDDEV(tx_count)                                         AS stddev_tx_per_user,
        PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY tx_count)  AS p50,
        PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY tx_count)  AS p90,
        PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY tx_count)  AS p99,
        MAX(tx_count)                                            AS max_tx_per_user
    FROM user_tx_counts
)

SELECT jsonb_pretty(
    jsonb_build_object(
        'user_degree', (SELECT jsonb_build_object(
            'mean',           ROUND(mean_degree::numeric, 4),
            'stddev',         ROUND(stddev_degree::numeric, 4),
            'p50',            p50_degree,
            'p90',            p90_degree,
            'p99',            p99_degree,
            'max',            max_degree,
            'power_law_gamma',ROUND(gamma_user_degree::numeric, 4)
        ) FROM user_degree_dist),

        'merchant_degree', (SELECT jsonb_build_object(
            'mean',           ROUND(mean_degree::numeric, 4),
            'stddev',         ROUND(stddev_degree::numeric, 4),
            'p50',            p50,
            'p90',            p90,
            'max',            max_degree,
            'power_law_gamma',ROUND(gamma_merchant_degree::numeric, 4)
        ) FROM merchant_degree_dist),

        'edge_weight', (SELECT jsonb_build_object(
            'mean',             ROUND(mean_edge_weight::numeric, 4),
            'stddev',           ROUND(stddev_edge_weight::numeric, 4),
            'p50',              p50_weight,
            'p90',              p90_weight,
            'p99',              p99_weight,
            'max',              max_weight,
            'geometric_p',      ROUND(geometric_p::numeric, 6),
            'repeat_edge_rate', ROUND(repeat_edge_rate::numeric, 4)
        ) FROM edge_weight_dist),

        'devices_per_user', (SELECT jsonb_build_object(
            'mean',               ROUND(mean_devices::numeric, 4),
            'stddev',             ROUND(stddev_devices::numeric, 4),
            'p50',                p50,
            'p90',                p90,
            'p99',                p99,
            'max',                max_devices,
            'multi_device_ratio', ROUND(multi_device_ratio::numeric, 4)
        ) FROM device_dist),

        'feature_covariance', (SELECT jsonb_build_object(
            'corr_amount_fee',      ROUND(corr_amount_fee::numeric, 6),
            'corr_amount_discount', ROUND(corr_amount_discount::numeric, 6),
            'corr_amount_markup',   ROUND(corr_amount_markup::numeric, 6),
            'mean_amount',          ROUND(mean_amount::numeric, 2),
            'mean_fee',             ROUND(mean_fee::numeric, 2),
            'std_amount',           ROUND(std_amount::numeric, 2),
            'std_fee',              ROUND(std_fee::numeric, 2)
        ) FROM covariance_features),

        'user_tx_frequency', (SELECT jsonb_build_object(
            'mean',   ROUND(mean_tx_per_user::numeric, 4),
            'stddev', ROUND(stddev_tx_per_user::numeric, 4),
            'p50',    p50,
            'p90',    p90,
            'p99',    p99,
            'max',    max_tx_per_user
        ) FROM user_tx_dist)
    )
);