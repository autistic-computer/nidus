// swift-tools-version: 6.2

import PackageDescription

let package = Package(
  name: "Nid",
  platforms: [.macOS(.v26)],
  products: [
    .executable(name: "nid-awdl-daemon", targets: ["AWDLDaemon"])
  ],
  targets: [
    .target(name: "AWDLGuardian", publicHeadersPath: "include"),
    .executableTarget(name: "AWDLDaemon", dependencies: ["AWDLGuardian"]),
  ]
)
