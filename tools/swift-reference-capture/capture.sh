#!/bin/zsh
# capture_swift.sh <out.png> [app args...] — launch the pinned Swift
# KeyInSight, wait for its window, capture it, quit.
set -u
S=${0:a:h}
# `windowid` is a build product, not committed — build it on demand.
[ -x "$S/windowid" ] || swiftc -O "$S/windowid.swift" -o "$S/windowid"
OUT=$1; shift
BIN=$S/../../keyinsight-swift-reference/.build/debug/KeyInSight
"$BIN" "$@" >/dev/null 2>&1 & PID=$!
for i in {1..40}; do INFO=$($S/windowid KeyInSight 2>/dev/null) && break; sleep 0.25; done
sleep 2.5   # let SwiftUI + WebView settle
WID=${INFO%% *}
screencapture -x -o -l "$WID" "$OUT"
kill $PID 2>/dev/null; wait $PID 2>/dev/null
echo "$OUT <- $*"
