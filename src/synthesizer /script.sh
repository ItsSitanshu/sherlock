#!/usr/bin/env bash
# =============================================================
# script.sh
# Runs all four PSQL extraction scripts and validates output.
#
# Usage:
#   export DATABASE_URL="postgresql://user:pass@host:5432/dbname"
#   ./script.sh
#
# Or pass DSN as argument:
#   ./script.sh "postgresql://user:pass@host:5432/dbname"
# =============================================================

set -euo pipefail

DSN="${1:-${DATABASE_URL:-}}"
if [[ -z "$DSN" ]]; then
  echo "ERROR: No DSN provided. Set DATABASE_URL or pass as argument."
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/scripts"
OUT_DIR="${SCRIPT_DIR}/../../../data/stats"
mkdir -p "$OUT_DIR"

run_sql() {
  local label="$1"
  local sql_file="$2"
  local out_file="$3"

  echo -n "  [$label] Running $sql_file ... "
  psql "$DSN" \
    --no-align \
    --tuples-only \
    --quiet \
    -f "$sql_file" \
    -o "$out_file"

  # Validate it's non-empty JSON
  if python3 -c "import json,sys; json.load(open('$out_file'))" 2>/dev/null; then
    local rows
    rows=$(wc -c < "$out_file")
    echo "OK (${rows} bytes)"
  else
    echo "WARN: output may not be valid JSON, check $out_file"
  fi
}

echo "======================================================"
echo "  Khalti Statistical Extraction Pipeline"
echo "======================================================"
echo "  DSN    : ${DSN//:*@/://***@}"   # mask password
echo "  Output : $OUT_DIR"
echo ""

run_sql "01/04" "$SCRIPT_DIR/distribution_metrics.sql" "$OUT_DIR/stats_distribution.json"
run_sql "02/04" "$SCRIPT_DIR/temporal_dynamics.sql"     "$OUT_DIR/stats_temporal.json"
run_sql "03/04" "$SCRIPT_DIR/extract_03_graph.sql"        "$OUT_DIR/stats_graph.json"
run_sql "04/04" "$SCRIPT_DIR/extract_04_entities.sql"     "$OUT_DIR/stats_entities.json"

echo ""
echo "======================================================"
echo "  All stats written to $OUT_DIR/"
ls -lh "$OUT_DIR/"
echo ""
echo "  Next step:"
echo "    cd .. && cargo run --release"
echo "======================================================"