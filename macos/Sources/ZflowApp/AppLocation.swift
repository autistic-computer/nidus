import Foundation

struct AppLocation: Equatable {
  var path: String
  var quarantined: Bool
  var readOnly: Bool
  var home: String

  static let main = AppLocation(bundle: .main)

  var needsMove: Bool {
    if path.contains("/AppTranslocation/") { return true }
    if path.hasPrefix("/Volumes/") && readOnly { return true }
    let folders = ["/Applications/", home + "/Applications/"]
    return quarantined && !folders.contains { path.hasPrefix($0) }
  }
}

extension AppLocation {
  init(bundle: Bundle) {
    let url = bundle.bundleURL
    let values = try? url.resourceValues(forKeys: [.volumeIsReadOnlyKey])
    self.init(
      path: url.path,
      quarantined: getxattr(url.path, "com.apple.quarantine", nil, 0, 0, 0) >= 0,
      readOnly: values?.volumeIsReadOnly ?? false,
      home: FileManager.default.homeDirectoryForCurrentUser.path)
  }
}
