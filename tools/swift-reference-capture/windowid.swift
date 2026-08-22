import CoreGraphics
import Foundation
// Print the CGWindowID of the first on-screen window whose owner name
// matches argv[1] (default "KeyInSight"), plus its bounds; exit 1 if none.
let want = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "KeyInSight"
let opts: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
guard let list = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] else { exit(1) }
for w in list {
    guard let owner = w[kCGWindowOwnerName as String] as? String, owner == want,
          let layer = w[kCGWindowLayer as String] as? Int, layer == 0,
          let id = w[kCGWindowNumber as String] as? Int,
          let b = w[kCGWindowBounds as String] as? [String: Any] else { continue }
    print("\(id) \(b["X"] ?? 0) \(b["Y"] ?? 0) \(b["Width"] ?? 0) \(b["Height"] ?? 0)")
    exit(0)
}
exit(1)
