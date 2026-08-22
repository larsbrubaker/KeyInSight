#!/bin/zsh
# capture_swift_demo.sh <outdir> — run the Swift --demo and grab the window
# every 0.5 s, logging demo: lines with frame numbers so frames can be
# matched to states afterwards.
set -u
S=${0:a:h}
OUT=$1; mkdir -p "$OUT"
BIN=$S/../../keyinsight-swift-reference/.build/debug/KeyInSight
"$BIN" --demo 2>&1 | grep --line-buffered "^demo:" > "$OUT/demo.log" & 
for i in {1..40}; do INFO=$($S/windowid KeyInSight 2>/dev/null) && break; sleep 0.25; done
WID=${INFO%% *}
n=0
while $S/windowid KeyInSight >/dev/null 2>&1; do
  n=$((n+1)); f=$(printf "%s/frame-%04d.png" "$OUT" $n)
  screencapture -x -o -l "$WID" "$f" 2>/dev/null
  echo "$(date +%s.%N) frame-$n $(tail -1 "$OUT/demo.log")" >> "$OUT/frames.log"
  sleep 0.5
done
wait
echo "frames: $n"; tail -3 "$OUT/demo.log"
