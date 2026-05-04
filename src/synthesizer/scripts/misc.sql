-- ============================================================
-- misc.sql
-- §2.4 Entity counts, categorical frequencies, wallet stats
--      status distributions, remittance corridors, service mix
-- Outputs: stats_entities.json
--
-- Run with:
--   psql $DSN -f misc.sql \
--     --no-align -t -o stats_entities.json
-- ============================================================

\set ON_ERROR_STOP on

-- ── User / Merchant counts ───────────────────────────────────────────────────
WITH role_counts AS (
    SELECT
        primary_role,
        COUNT(*) AS cnt
    FROM autho_user
    WHERE is_deleted = FALSE
    GROUP BY primary_role
),

-- ── Profile status breakdown ─────────────────────────────────────────────────
profile_status AS (
    SELECT
        profile_status,
        COUNT(*) AS cnt,
        COUNT(*)::float / SUM(COUNT(*)) OVER () AS ratio
    FROM autho_user
    WHERE is_deleted = FALSE
    GROUP BY profile_status
),

-- ── KYC / verification rates ─────────────────────────────────────────────────
verification_stats AS (
    SELECT
        COUNT(*) FILTER (WHERE is_verified)::float / COUNT(*) AS verified_rate,
        COUNT(*) FILTER (WHERE has_kyc)::float      / COUNT(*) AS kyc_rate,
        COUNT(*) FILTER (WHERE is_email_verified)::float / COUNT(*) AS email_verified_rate,
        COUNT(*) FILTER (WHERE is_active)::float    / COUNT(*) AS active_rate,
        AVG(EXTRACT(YEAR FROM AGE(dob)))             AS mean_age_years,
        STDDEV(EXTRACT(YEAR FROM AGE(dob)))          AS stddev_age_years
    FROM autho_user
    WHERE is_deleted = FALSE AND dob IS NOT NULL
),

-- ── Gender distribution ──────────────────────────────────────────────────────
gender_dist AS (
    SELECT
        gender_id,
        COUNT(*) AS cnt,
        COUNT(*)::float / SUM(COUNT(*)) OVER () AS ratio
    FROM autho_user
    WHERE is_deleted = FALSE AND gender_id IS NOT NULL
    GROUP BY gender_id
),

-- ── District distribution (top 20) ──────────────────────────────────────────
district_dist AS (
    SELECT
        district_id,
        COUNT(*) AS cnt,
        COUNT(*)::float / SUM(COUNT(*)) OVER () AS ratio
    FROM autho_user
    WHERE is_deleted = FALSE AND district_id IS NOT NULL
    GROUP BY district_id
    ORDER BY cnt DESC
    LIMIT 20
),

-- ── Wallet balance distribution ──────────────────────────────────────────────
wallet_dist AS (
    SELECT
        AVG(balance)                                              AS mean_balance,
        STDDEV(balance)                                           AS stddev_balance,
        AVG(LN(NULLIF(balance, 0)))                              AS lognorm_mu,
        STDDEV(LN(NULLIF(balance, 0)))                           AS lognorm_sigma,
        PERCENTILE_CONT(0.10) WITHIN GROUP (ORDER BY balance)    AS p10,
        PERCENTILE_CONT(0.25) WITHIN GROUP (ORDER BY balance)    AS p25,
        PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY balance)    AS p50,
        PERCENTILE_CONT(0.75) WITHIN GROUP (ORDER BY balance)    AS p75,
        PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY balance)    AS p90,
        PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY balance)    AS p99,
        AVG(hold_balance)::float / NULLIF(AVG(balance), 0)       AS mean_hold_ratio
    FROM gateway_walletbalance
),

-- ── Transaction status distribution ─────────────────────────────────────────
qr_status AS (
    SELECT status, COUNT(*) AS cnt,
           COUNT(*)::float / SUM(COUNT(*)) OVER () AS ratio
    FROM qrapp_fonepaytransaction GROUP BY status
),
svc_status AS (
    SELECT status, COUNT(*) AS cnt,
           COUNT(*)::float / SUM(COUNT(*)) OVER () AS ratio
    FROM service_servicelog GROUP BY status
),
rem_status AS (
    SELECT status, COUNT(*) AS cnt,
           COUNT(*)::float / SUM(COUNT(*)) OVER () AS ratio
    FROM remittance_remittance GROUP BY status
),
dis_status AS (
    SELECT status, COUNT(*) AS cnt,
           COUNT(*)::float / SUM(COUNT(*)) OVER () AS ratio
    FROM disbursement_transaction GROUP BY status
),

-- ── Remittance: corridor and exchange rate stats ──────────────────────────────
remit_corridors AS (
    SELECT
        sender_country,
        COUNT(*)                                              AS n,
        COUNT(*)::float / SUM(COUNT(*)) OVER ()              AS weight,
        AVG(exchange_rate)                                   AS mean_rate,
        STDDEV(exchange_rate)                                AS stddev_rate,
        AVG(amount)                                          AS mean_amount,
        AVG(service_charge)                                  AS mean_charge
    FROM remittance_remittance
    GROUP BY sender_country
    ORDER BY n DESC
    LIMIT 15
),

-- ── Service mix (top services by volume) ─────────────────────────────────────
service_mix AS (
    SELECT
        service_id,
        COUNT(*)                                            AS n,
        COUNT(*)::float / SUM(COUNT(*)) OVER ()            AS weight,
        AVG(amount)                                        AS mean_amount,
        AVG(service_charge)                                AS mean_charge
    FROM service_servicelog
    GROUP BY service_id
    ORDER BY n DESC
    LIMIT 30
),

-- ── on_us rate in QR ─────────────────────────────────────────────────────────
on_us_stats AS (
    SELECT
        COUNT(*) FILTER (WHERE on_us)::float / COUNT(*)  AS on_us_rate,
        AVG(amount) FILTER (WHERE on_us)                 AS mean_amount_on_us,
        AVG(amount) FILTER (WHERE NOT on_us)             AS mean_amount_off_us
    FROM qrapp_fonepaytransaction
),

-- ── Fee / markup rates ────────────────────────────────────────────────────────
fee_rates AS (
    SELECT
        AVG(fee_amount::float / NULLIF(amount, 0))       AS mean_fee_rate,
        STDDEV(fee_amount::float / NULLIF(amount, 0))    AS stddev_fee_rate,
        AVG(discount_amount::float / NULLIF(amount, 0))  AS mean_discount_rate,
        AVG(markup_amount::float / NULLIF(amount, 0))    AS mean_markup_rate,
        CORR(amount::float, fee_amount::float)           AS corr_amount_fee
    FROM qrapp_fonepaytransaction
    WHERE amount > 0
)

SELECT jsonb_pretty(
    jsonb_build_object(
        'entity_counts',      (SELECT jsonb_object_agg(primary_role, cnt) FROM role_counts),
        'profile_status',     (SELECT jsonb_agg(jsonb_build_object(
                                  'status', profile_status, 'cnt', cnt,
                                  'ratio', ROUND(ratio::numeric, 4))) FROM profile_status),
        'verification',       (SELECT jsonb_build_object(
                                  'verified_rate',       ROUND(verified_rate::numeric, 4),
                                  'kyc_rate',            ROUND(kyc_rate::numeric, 4),
                                  'email_verified_rate', ROUND(email_verified_rate::numeric, 4),
                                  'active_rate',         ROUND(active_rate::numeric, 4),
                                  'mean_age_years',      ROUND(mean_age_years::numeric, 2),
                                  'stddev_age_years',    ROUND(stddev_age_years::numeric, 2)
                                ) FROM verification_stats),
        'gender',             (SELECT jsonb_agg(jsonb_build_object(
                                  'gender_id', gender_id, 'cnt', cnt,
                                  'ratio', ROUND(ratio::numeric, 4))
                                  ORDER BY gender_id) FROM gender_dist),
        'top_districts',      (SELECT jsonb_agg(jsonb_build_object(
                                  'district_id', district_id, 'cnt', cnt,
                                  'ratio', ROUND(ratio::numeric, 4))
                                  ORDER BY cnt DESC) FROM district_dist),
        'wallet_balance',     (SELECT jsonb_build_object(
                                  'mean',          ROUND(mean_balance::numeric, 2),
                                  'stddev',        ROUND(stddev_balance::numeric, 2),
                                  'lognorm_mu',    ROUND(lognorm_mu::numeric, 6),
                                  'lognorm_sigma', ROUND(lognorm_sigma::numeric, 6),
                                  'p10', p10, 'p25', p25, 'p50', p50,
                                  'p75', p75, 'p90', p90, 'p99', p99,
                                  'mean_hold_ratio', ROUND(mean_hold_ratio::numeric, 4)
                                ) FROM wallet_dist),
        'status_dist', jsonb_build_object(
            'qr_payment',    (SELECT jsonb_agg(jsonb_build_object('status',status,'ratio',ROUND(ratio::numeric,4))) FROM qr_status),
            'utilities',     (SELECT jsonb_agg(jsonb_build_object('status',status,'ratio',ROUND(ratio::numeric,4))) FROM svc_status),
            'remittance',    (SELECT jsonb_agg(jsonb_build_object('status',status,'ratio',ROUND(ratio::numeric,4))) FROM rem_status),
            'disbursement',  (SELECT jsonb_agg(jsonb_build_object('status',status,'ratio',ROUND(ratio::numeric,4))) FROM dis_status)
        ),
        'remittance_corridors', (SELECT jsonb_agg(jsonb_build_object(
                                    'country', sender_country, 'n', n,
                                    'weight', ROUND(weight::numeric, 4),
                                    'mean_rate', ROUND(mean_rate::numeric, 4),
                                    'stddev_rate', ROUND(stddev_rate::numeric, 4),
                                    'mean_amount', ROUND(mean_amount::numeric, 2),
                                    'mean_charge', ROUND(mean_charge::numeric, 2)
                                  ) ORDER BY n DESC) FROM remit_corridors),
        'service_mix',          (SELECT jsonb_agg(jsonb_build_object(
                                    'service_id', service_id,
                                    'weight', ROUND(weight::numeric, 6),
                                    'mean_amount', ROUND(mean_amount::numeric, 2),
                                    'mean_charge', ROUND(mean_charge::numeric, 2)
                                  ) ORDER BY n DESC) FROM service_mix),
        'on_us',               (SELECT jsonb_build_object(
                                    'rate',              ROUND(on_us_rate::numeric, 4),
                                    'mean_amount_on_us', ROUND(mean_amount_on_us::numeric, 2),
                                    'mean_amount_off_us',ROUND(mean_amount_off_us::numeric, 2)
                                  ) FROM on_us_stats),
        'fee_rates',           (SELECT jsonb_build_object(
                                    'mean_fee_rate',      ROUND(mean_fee_rate::numeric, 6),
                                    'stddev_fee_rate',    ROUND(stddev_fee_rate::numeric, 6),
                                    'mean_discount_rate', ROUND(mean_discount_rate::numeric, 6),
                                    'mean_markup_rate',   ROUND(mean_markup_rate::numeric, 6),
                                    'corr_amount_fee',    ROUND(corr_amount_fee::numeric, 6)
                                  ) FROM fee_rates)
    )
);