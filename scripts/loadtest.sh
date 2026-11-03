#!/usr/bin/env bash
# SFU load test: one room with N participants: min(N,10) publish simulcast video (cameras
# on), the rest subscribe with a 3x3 grid layout (what our UI shows). Samples the
# livekit-server process CPU/memory while it runs.
#
#   scripts/loadtest.sh <participants> [duration]     e.g. scripts/loadtest.sh 25 60s
#
# Needs: lk (livekit-cli) and a running livekit-server (`--dev` uses devkey/secret).
# Note: when lk runs on the same machine as the server, both compete for CPU, so numbers
# are a lower bound for that machine, not a production capacity figure.
set -euo pipefail
n=${1:?participants}; dur=${2:-60s}
pubs=$(( n < 10 ? n : 10 )); subs=$(( n - pubs ))
out=${OUT:-loadtest-results}; mkdir -p "$out"
pid=$(pgrep -x livekit-server | head -1) || { echo "livekit-server not running" >&2; exit 1; }

samples="$out/$n.samples"; : > "$samples"
( while kill -0 "$pid" 2>/dev/null; do ps -o %cpu=,rss= -p "$pid" >> "$samples"; sleep 2; done ) &
sampler=$!
trap 'kill $sampler 2>/dev/null || true' EXIT

lk load-test --url "${LIVEKIT_URL:-http://127.0.0.1:7880}" \
  --api-key "${LIVEKIT_API_KEY:-devkey}" --api-secret "${LIVEKIT_API_SECRET:-secret}" \
  --room "load-$n-$$" --video-publishers "$pubs" --subscribers "$subs" --layout 3x3 --video-resolution medium \
  --num-per-second 5 --duration "$dur" 2>&1 | tee "$out/$n.log"

kill $sampler 2>/dev/null || true
awk -v n="$n" '{c+=$1; if($1>mc)mc=$1; if($2>mr)mr=$2; k++}
  END{printf "N=%s  server CPU avg %.0f%% peak %.0f%% (100%% = 1 core)  peak RSS %.0f MB\n", n, c/k, mc, mr/1024}' \
  "$samples" | tee "$out/$n.summary"
