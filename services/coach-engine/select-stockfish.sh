#!/bin/sh
set -eu

generic=/usr/local/bin/stockfish
avx2=/usr/local/bin/stockfish-avx2
selected=${STOCKFISH_PATH:-$generic}

if [ "$selected" = "$generic" ] && grep -qw avx2 /proc/cpuinfo; then
  selected=$avx2
fi

export STOCKFISH_PATH="$selected"
printf '{"level":"info","event":"stockfish_binary_selected","path":"%s"}\n' "$selected"
exec "$@"
