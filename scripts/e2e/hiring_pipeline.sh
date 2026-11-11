#!/usr/bin/env bash
set -euo pipefail

BASE_URL=${BASE_URL:-http://localhost:8000}
JAR_DIR=${JAR_DIR:-/tmp/hustle-e2e-jars}
mkdir -p "$JAR_DIR"

need() { command -v "$1" >/dev/null 2>&1 || { echo "missing dependency: $1" >&2; exit 1; }; }
need curl
need python3

api() {
  local method=$1 path=$2 jar=${3:-} body=${4:-}
  local n=$#
  if [ "$n" -ge 4 ]; then shift 4; else shift "$n"; fi
  local args=(-sS -X "$method" "$BASE_URL$path" -H 'Content-Type: application/json')
  [ -n "$jar" ] && args+=(-b "$jar" -c "$jar")
  [ -n "$body" ] && args+=(-d "$body")
  curl "${args[@]}" "$@"
}

login() {
  local jar=$1 email=$2 pass=$3
  rm -f "$jar"
  api POST /api/auth/login "$jar" "{\"email\":\"$email\",\"password\":\"$pass\"}" -o /dev/null -w '%{http_code}' | grep -q '^200$' || { echo "login failed: $email" >&2; exit 1; }
}

json_get() { python3 -c "import json,sys; d=json.load(sys.stdin); print($1)"; }

echo "[1] login seeded users"
login "$JAR_DIR/h1" "h1@example.com" "hirer1"
login "$JAR_DIR/w1" "w1@example.com" "worker1"
H1_ID=$(api GET /api/users/me "$JAR_DIR/h1" | json_get "d['id']")
W1_ID=$(api GET /api/users/me "$JAR_DIR/w1" | json_get "d['id']")
echo "  h1=$H1_ID w1=$W1_ID"

echo "[2] create gig"
GIG=$(api POST /api/gigs "$JAR_DIR/h1" '{"title":"E2E pipeline gig","description":"ops hardening check","collar":"blue","location":"Test City"}')
GID=$(echo "$GIG" | json_get "d['id']")
[ "$(echo "$GIG" | json_get "d['status']")" = "open" ] || { echo "expected open gig" >&2; exit 1; }

echo "[3] worker applies"
CODE=$(api POST "/api/gigs/$GID/apply" "$JAR_DIR/w1" '{"message":"ready"}' -o /dev/null -w '%{http_code}')
[ "$CODE" = "201" ] || { echo "apply expected 201 got $CODE" >&2; exit 1; }

echo "[4] poster accepts applicant"
ACCEPT=$(api POST "/api/gigs/$GID/applicants/$W1_ID/accept" "$JAR_DIR/h1")
[ "$(echo "$ACCEPT" | json_get "d['status']")" = "in_progress" ] || { echo "expected in_progress" >&2; exit 1; }
[ "$(echo "$ACCEPT" | json_get "d['hired_worker_id']")" = "$W1_ID" ] || { echo "expected hired worker $W1_ID" >&2; exit 1; }

echo "[5] both confirm completion"
C1=$(api POST "/api/gigs/$GID/complete" "$JAR_DIR/h1")
[ "$(echo "$C1" | json_get "d['status']")" = "in_progress" ] || { echo "expected still in_progress after poster confirm" >&2; exit 1; }
C2=$(api POST "/api/gigs/$GID/complete" "$JAR_DIR/w1")
[ "$(echo "$C2" | json_get "d['status']")" = "completed" ] || { echo "expected completed after both confirm" >&2; exit 1; }

echo "[6] both rate"
R1_CODE=$(api POST "/api/gigs/$GID/rating" "$JAR_DIR/h1" '{"rating":5,"comment":"great"}' -o /dev/null -w '%{http_code}')
[ "$R1_CODE" = "201" ] || { echo "poster rating expected 201 got $R1_CODE" >&2; exit 1; }
R2_CODE=$(api POST "/api/gigs/$GID/rating" "$JAR_DIR/w1" '{"rating":4,"comment":"good"}' -o /dev/null -w '%{http_code}')
[ "$R2_CODE" = "201" ] || { echo "worker rating expected 201 got $R2_CODE" >&2; exit 1; }

echo "[7] ratings visible on profiles"
W1_R=$(api GET "/api/users/$W1_ID/ratings" "$JAR_DIR/h1")
[ "$(echo "$W1_R" | json_get "d['summary']['count']")" -ge 1 ] 2>/dev/null || { echo "expected worker ratings count >=1" >&2; exit 1; }
H1_R=$(api GET "/api/users/$H1_ID/ratings" "$JAR_DIR/w1")
[ "$(echo "$H1_R" | json_get "d['summary']['count']")" -ge 1 ] 2>/dev/null || { echo "expected hirer ratings count >=1" >&2; exit 1; }

echo "HIRING_PIPELINE_E2E_OK"
