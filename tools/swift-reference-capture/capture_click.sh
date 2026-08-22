#!/bin/zsh
# capture_swift_click.sh <out.png> "<x,y>[;<x,y>...]" [app args...]
# Launch the Swift app, click the given window-relative logical points
# (title bar included, window is 1280x572 logical) via System Events, capture.
set -u
S=${0:a:h}
OUT=$1; CLICKS=$2; shift 2
BIN=$S/../../keyinsight-swift-reference/.build/debug/KeyInSight
"$BIN" "$@" >/dev/null 2>&1 & PID=$!
for i in {1..40}; do INFO=$($S/windowid KeyInSight 2>/dev/null) && break; sleep 0.25; done
sleep 2
read WID WX WY WW WH <<< "$INFO"
for c in ${(s:;:)CLICKS}; do
  x=${c%%,*}; y=${c##*,}
  osascript -e "tell application \"System Events\" to click at {$((WX + x)), $((WY + y))}" >/dev/null
  sleep 1.2
done
sleep 1
screencapture -x -o -l "$WID" "$OUT"
kill $PID 2>/dev/null; wait $PID 2>/dev/null
echo "$OUT <- clicks [$CLICKS] $*"
