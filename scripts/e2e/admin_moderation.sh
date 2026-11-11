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

echo "[1] login"
login "$JAR_DIR/boss" "boss@example.com" "secret123"
login "$JAR_DIR/pleb" "pleb@example.com" "secret123"
PLEB_ID=$(api GET /api/users/me "$JAR_DIR/pleb" | json_get "d['id']")

echo "[2] admin stats/users/audit reachable"
api GET /api/admin/stats "$JAR_DIR/boss" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "user_count" in d' 
api GET "/api/admin/users?per_page=1" "$JAR_DIR/boss" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "items" in d'
api GET "/api/admin/audit?per_page=1" "$JAR_DIR/boss" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "items" in d'

echo "[3] block/unblock flow"
# pick another user (boss) as target for block by pleb
BOSS_ID=$(api GET /api/users/me "$JAR_DIR/boss" | json_get "d['id']")
api DELETE "/api/users/$BOSS_ID/block" "$JAR_DIR/pleb" "" -o /dev/null -w '' >/dev/null 2>&1 || true
CODE=$(api POST "/api/users/$BOSS_ID/block" "$JAR_DIR/pleb" "" -o /dev/null -w '%{http_code}')
[ "$CODE" = "204" ] || { echo "block expected 204 got $CODE" >&2; exit 1; }
api GET /api/users/me/blocks "$JAR_DIR/pleb" | python3 -c "import json,sys; d=json.load(sys.stdin); assert any(u.get('id')==$BOSS_ID for u in d)"
CODE=$(api DELETE "/api/users/$BOSS_ID/block" "$JAR_DIR/pleb" "" -o /dev/null -w '%{http_code}')
[ "$CODE" = "204" ] || { echo "unblock expected 204 got $CODE" >&2; exit 1; }

echo "[4] report + admin resolve flow"
POST=$(api POST /api/posts "$JAR_DIR/pleb" '{"content":"e2e moderation report target"}')
PID=$(echo "$POST" | json_get "d['id']")
REP=$(api POST /api/reports "$JAR_DIR/pleb" "{\"target_type\":\"post\",\"target_id\":$PID,\"reason\":\"spam\"}")
RID=$(echo "$REP" | json_get "d['id']")
RES=$(api PATCH "/api/admin/reports/$RID" "$JAR_DIR/boss" '{"status":"resolved","delete_target":true,"admin_note":"e2e"}')
[ "$(echo "$RES" | json_get "d['status']")" = "resolved" ] || { echo "expected resolved report" >&2; exit 1; }
# deleted post should be gone from admin search
LIST=$(api GET "/api/admin/posts?q=e2e%20moderation%20report%20target" "$JAR_DIR/boss")
echo "$LIST" | python3 -c "import json,sys; d=json.load(sys.stdin); assert not any(p['id']==$PID for p in d.get('items',[]))" || { echo "expected reported post deleted" >&2; exit 1; }

echo "[5] audit records present"
api GET "/api/admin/audit?per_page=10&target_type=report" "$JAR_DIR/boss" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("total",0) >= 1'

echo "ADMIN_MODERATION_E2E_OK"
