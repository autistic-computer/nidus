import Foundation
import ZflowCore

struct Computer: Codable, Identifiable, Equatable, Sendable {
  var id: String
  var label: String
  var peer: String?
  var display: DisplayInfo?
  var x: Int
  var y: Int
  var width: Int
  var height: Int
}
struct DisplayInfo: Codable, Equatable, Sendable {
  var id: String
  var name: String
  var active: Bool
  var bounds: Bounds?
  var widthMm: Int?
  var heightMm: Int?

  struct Bounds: Codable, Equatable, Sendable {
    var width: Int
    var height: Int
  }
}
struct Layout: Decodable, Equatable, Sendable { var monitors: [Computer] }
struct Seam: Decodable, Equatable, Sendable {
  var a: String
  var b: String
  var vertical: Bool
  var at: Int
  var start: Int
  var end: Int
  var sameComputer: Bool
}
struct Preview: Decodable, Equatable, Sendable {
  var x: Int
  var y: Int
  var seams: [Seam]
}
struct Unplaced: Decodable, Identifiable, Equatable, Sendable {
  var id: String
  var name: String
  var os: Os?
  var mark: String?
  var version: String?
  var state: State
  var via: Via

  enum State: String, Decodable, Sendable {
    case identifying, ready
    case differentVersion = "different_version"
    case duplicateName = "duplicate_name"
  }
  enum Via: String, Decodable, Sendable { case mdns, address }
  var placeable: Bool { state == .ready || state == .duplicateName }
}
enum Os: String, Decodable, Sendable { case linux, macos, windows }
struct PairingWindow: Decodable, Equatable, Sendable {
  var state: State
  var secondsLeft: Int?
  var holding: Holding?
  var reason: Reason?

  enum State: String, Decodable, Sendable { case never, eligible, open, closed }
  enum Reason: String, Decodable, Sendable {
    case accepted, expired, rival, restarted, failed
    case hadPeers = "had_peers"
  }
  struct Holding: Decodable, Equatable, Sendable {
    var name: String
    var mark: String
    var msLeft: Int
  }
}
struct Notice: Decodable, Identifiable, Equatable, Sendable {
  var id: UInt64
  var kind: Kind
  var name: String
  var mark: String

  enum Kind: String, Decodable, Sendable { case joined }
}
struct Snapshot: Decodable, Equatable, Sendable {
  var status: Status
  var sharing: Bool?
  var health: [Health]
  var pairingWindow: PairingWindow
  var layout: Layout?
  var seams: [Seam]
  var ownMark: String?
  var unplaced: [Unplaced]
  var peers: [Peer]
  var pauseAtEdges: Bool?
  var shareClipboard: Bool?
  var configPath: String
  var notices: [Notice]
  var platform: MacPlatform

  var problems: [Health] { health.filter { $0.level != .ok } }
}
struct Status: Decodable, Equatable, Sendable {
  var state: State
  var peer: String?
  var title: String

  enum State: String, Decodable, Sendable {
    case ready, controlling, controlled, paused, checking, setup, attention
  }
}
struct Peer: Decodable, Identifiable, Equatable, Sendable {
  var name: String
  var state: State
  var detail: String
  var allowControl: Bool
  var keyboard: KeyboardMode
  var reverseScroll: Bool
  var mark: String
  var id: String { name }

  enum State: String, Decodable, Sendable {
    case paired, connecting, connected, unreachable
    case controllingThis = "controlling_this"
    case controlledFromHere = "controlled_from_here"
  }
}
enum KeyboardMode: String, Codable, CaseIterable, Sendable {
  case standard
  case pcPositions = "pc_positions"
  case mac
}
struct Health: Decodable, Identifiable, Equatable, Sendable {
  var id: String
  var level: Level
  var title: String
  var detail: String
  var action: HealthAction?

  enum Level: String, Decodable, Sendable { case ok, warning, error }
}
struct HealthAction: Decodable, Equatable, Sendable {
  var label: String
  var command: String
}
struct MacPlatform: Decodable, Equatable, Sendable {
  var accessibility: Bool
  var localNetwork: LocalNetwork
  var blockAwdl: Bool

  enum LocalNetwork: String, Decodable, Sendable { case allowed, blocked, unknown }
}
struct CoreRequest: Encodable, Sendable {
  var command: String
  var enabled: Bool?
  var ready: Bool?
  var id: String?
  var x: Int?
  var y: Int?
  var tolerance: Int?
  var address: String?
  var name: String?
  var allowControl: Bool?
  var keyboard: KeyboardMode?
  var reverseScroll: Bool?
  var pauseAtEdges: Bool?
  var share: Bool?
  var computer: Bool?
}
private struct CoreResponse<Answer: Decodable>: Decodable {
  var snapshot: Answer?
  var error: String?
}
struct AppError: LocalizedError {
  let message: String
  var errorDescription: String? { message }
}

private final class CoreHandle: @unchecked Sendable {
  let pointer: OpaquePointer
  init(path: String) throws {
    var error: UnsafeMutablePointer<CChar>?
    let pointer = path.withCString { zflow_app_create($0, &error) }
    defer { zflow_string_free(error) }
    guard let pointer else {
      throw AppError(message: error.map { String(cString: $0) } ?? "Could not start sharing")
    }
    self.pointer = pointer
  }
  deinit { zflow_app_destroy(pointer) }
}
final class LensHandle: Sendable {
  private weak let core: CoreHandle?
  fileprivate init(_ core: CoreHandle) { self.core = core }
  func callAsFunction<T>(_ body: (OpaquePointer) -> T) -> T? {
    guard let core else { return nil }
    return withExtendedLifetime(core) { body(core.pointer) }
  }
}
actor CoreBridge {
  let path: String
  private var handle: CoreHandle?
  private var closed = false
  init(path: String) { self.path = path }
  func request(_ request: CoreRequest) throws -> Snapshot {
    guard let snapshot = try call(request, answer: Snapshot.self) else {
      throw AppError(message: "The sharing engine returned no status")
    }
    return snapshot
  }
  func preview(_ request: CoreRequest) throws -> Preview? {
    try call(request, answer: Preview.self)
  }
  private func call<Answer: Decodable>(_ request: CoreRequest, answer: Answer.Type) throws
    -> Answer?
  {
    let handle = try open()
    let encoder = JSONEncoder()
    encoder.keyEncodingStrategy = .convertToSnakeCase
    let data = try encoder.encode(request)
    let text = String(decoding: data, as: UTF8.self)
    let response = text.withCString { zflow_app_request(handle.pointer, $0) }
    guard let response else { throw AppError(message: "The sharing engine returned no response") }
    defer { zflow_string_free(response) }
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let result = try decoder.decode(
      CoreResponse<Answer>.self, from: Data(String(cString: response).utf8))
    if let error = result.error { throw AppError(message: error) }
    return result.snapshot
  }
  private func open() throws -> CoreHandle {
    guard !closed else { throw AppError(message: "The sharing engine has stopped") }
    if let handle { return handle }
    let handle = try CoreHandle(path: path)
    self.handle = handle
    return handle
  }
  func lens() throws -> LensHandle { LensHandle(try open()) }
  func shutdown() {
    closed = true
    handle = nil
  }
}
