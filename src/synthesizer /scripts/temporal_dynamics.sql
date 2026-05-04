-- ============================================================
-- temporal_dynamics.sql
-- §2.2 Temporal Dynamics
-- Outputs: stats_temporal.json
--
-- Run with:
--   psql $DSN -f temporal_dynamics.sql \
--     --no-align -t -o stats_temporal.json
-- ============================================================

\set ON_ERROR_STOP on

-- ── Hour-of-day distribution (Nepal time = UTC+05:45) ────────────────────────
WITH nepal_tz AS (
    SELECT
        id,
        user_id,
        created_on AT TIME ZONE 'Asia/Kathmandu' AS created_npt,
        transaction_id
    FROM gateway_wallethistory
    WHERE created_on IS NOT NULL
),

hour_dist AS (
    SELECT
        EXTRACT(HOUR FROM created_npt)::int AS hour,
        COUNT(*)                            AS tx_count,
        COUNT(*)::float / SUM(COUNT(*)) OVER () AS weight
    FROM nepal_tz
    GROUP BY hour
    ORDER BY hour
),

-- ── Day-of-week distribution ─────────────────────────────────────────────────
dow_dist AS (
    SELECT
        -- 0=Sunday … 6=Saturday (PostgreSQL EXTRACT DOW)
        EXTRACT(DOW FROM created_npt)::int  AS dow,
        TO_CHAR(created_npt, 'Day')         AS dow_name,
        COUNT(*)                            AS tx_count,
        COUNT(*)::float / SUM(COUNT(*)) OVER () AS weight
    FROM nepal_tz
    GROUP BY dow, dow_name
    ORDER BY dow
),

-- ── Inter-arrival time per user (seconds between consecutive tx) ─────────────
user_gaps AS (
    SELECT
        user_id,
        EXTRACT(EPOCH FROM (
            created_npt - LAG(created_npt) OVER (PARTITION BY user_id ORDER BY created_npt)
        ))::float AS gap_seconds
    FROM nepal_tz
),

interarrival AS (
    SELECT
        AVG(gap_seconds)                                        AS mean_gap_s,
        STDDEV(gap_seconds)                                     AS stddev_gap_s,
        PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY gap_seconds) AS p50_gap_s,
        PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY gap_seconds) AS p90_gap_s,
        PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY gap_seconds) AS p99_gap_s,
        -- Exponential rate (λ = 1/mean)
        1.0 / NULLIF(AVG(gap_seconds), 0)                      AS lambda_per_second
    FROM user_gaps
    WHERE gap_seconds IS NOT NULL AND gap_seconds > 0
),

-- ── Session / burst detection (gaps < 300s = same session) ───────────────────
session_flags AS (
    SELECT
        user_id,
        created_npt,
        CASE WHEN gap_seconds < 300 THEN 1 ELSE 0 END AS in_burst
    FROM user_gaps
    WHERE gap_seconds IS NOT NULL
),

burst_stats AS (
    SELECT
        SUM(in_burst)::float / NULLIF(COUNT(*), 0) AS burst_ratio,
        COUNT(DISTINCT user_id)                     AS users_with_gaps
    FROM session_flags
),

-- ── Tx per user per day (periodicity measure) ────────────────────────────────
daily_per_user AS (
    SELECT
        user_id,
        DATE(created_npt)             AS tx_date,
        COUNT(*)                      AS daily_count
    FROM nepal_tz
    GROUP BY user_id, tx_date
),

daily_stats AS (
    SELECT
        AVG(daily_count)     AS mean_tx_per_user_day,
        STDDEV(daily_count)  AS stddev_tx_per_user_day,
        MAX(daily_count)     AS max_tx_per_user_day,
        PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY daily_count) AS p95_tx_per_user_day
    FROM daily_per_user
)

SELECT jsonb_pretty(
    jsonb_build_object(
        'hour_of_day',   (SELECT jsonb_agg(jsonb_build_object(
                            'hour', hour,
                            'tx_count', tx_count,
                            'weight', ROUND(weight::numeric, 6)
                          ) ORDER BY hour) FROM hour_dist),
        'day_of_week',   (SELECT jsonb_agg(jsonb_build_object(
                            'dow', dow,
                            'name', TRIM(dow_name),
                            'tx_count', tx_count,
                            'weight', ROUND(weight::numeric, 6)
                          ) ORDER BY dow) FROM dow_dist),
        'interarrival',  (SELECT jsonb_build_object(
                            'mean_gap_seconds',   ROUND(mean_gap_s::numeric, 2),
                            'stddev_gap_seconds', ROUND(stddev_gap_s::numeric, 2),
                            'p50_gap_seconds',    ROUND(p50_gap_s::numeric, 2),
                            'p90_gap_seconds',    ROUND(p90_gap_s::numeric, 2),
                            'p99_gap_seconds',    ROUND(p99_gap_s::numeric, 2),
                            'lambda_per_second',  ROUND(lambda_per_second::numeric, 8)
                          ) FROM interarrival),
        'session',       (SELECT jsonb_build_object(
                            'burst_ratio',       ROUND(burst_ratio::numeric, 4),
                            'users_with_gap_data', users_with_gaps
                          ) FROM burst_stats),
        'daily_per_user', (SELECT jsonb_build_object(
                            'mean',  ROUND(mean_tx_per_user_day::numeric, 4),
                            'stddev',ROUND(stddev_tx_per_user_day::numeric, 4),
                            'max',   max_tx_per_user_day,
                            'p95',   ROUND(p95_tx_per_user_day::numeric, 2)
                          ) FROM daily_stats)
    )
);