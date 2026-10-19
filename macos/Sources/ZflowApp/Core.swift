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
  /// Its pixels, as macOS or the other computer reports them.
  var bounds: Bounds?
  /// Zero when the system cannot say.
  var widthMm: Int?
  var heightMm: Int?

  struct Bounds: Codable, Equatable, Sendable {
    var width: Int
    var height: Int
  }
}
struct Layout: Decodable, Equatable, Sendable { var monitors: [Computer] }
/// src/app/layout_model.rs Seam: where two screens touch, in layout units.
struct Seam: Decodable, Equatable, Sendable {
  /// The screen left of the line, or above it.
  var a: String
  var b: String
  /// Whether the screens sit side by side, so the line stands upright.
  var vertical: Bool
  /// The line's x when it stands upright, else its y.
  var at: Int
  var start: Int
  var end: Int
  /// Both screens are one computer's, whose display settings move the
  /// pointer between them. The pointer crosses to another computer only at
  /// the other seams.
  var sameComputer: Bool
}
/// src/app/layout_model.rs Preview: where a move would land, for a drag to show.
struct Preview: Decodable, Equatable, Sendable {
  var x: Int
  var y: Int
  var seams: [Seam]
}
/// src/neighbors.rs Unplaced: a computer found around this Mac that is not
/// on the board yet, for the shelf.
struct Unplaced: Decodable, Identifiable, Equatable, Sendable {
  /// What `place` takes once its hello proved its key.
  var id: String
  var name: String
  var os: Os?
  /// Its key's mark; nil until its hello comes back.
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
  /// Only a computer whose key is known, on this version, can be placed.
  var placeable: Bool { state == .ready || state == .duplicateName }
}
enum Os: String, Decodable, Sendable { case linux, macos, windows }
/// src/pairing_window.rs View: the ten minutes in which a fresh install
/// lets the one new computer around join by itself.
struct PairingWindow: Decodable, Equatable, Sendable {
  var state: State
  var secondsLeft: Int?
  /// The computer about to join.
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
/// src/hello.rs Notice: something to tell people once.
struct Notice: Decodable, Identifiable, Equatable, Sendable {
  /// Grows with each notice, so each is posted once.
  var id: UInt64
  var kind: Kind
  var name: String
  /// That computer's key mark, so a person can tell it from a namesake.
  var mark: String

  enum Kind: String, Decodable, Sendable { case joined }
}
/// src/app/api.rs Snapshot, which the GNOME settings window shows too. The
/// shared rows come first, in window order, then the ones for this Mac.
struct Snapshot: Decodable, Equatable, Sendable {
  var status: Status
  /// Nil while the part that shares input cannot be reached.
  var sharing: Bool?
  var health: [Health]
  var pairingWindow: PairingWindow
  var layout: Layout?
  /// Where the layout's screens touch.
  var seams: [Seam]
  /// This Mac's key mark, drawn on its tile.
  var ownMark: String?
  /// Computers found but not on the board yet.
  var unplaced: [Unplaced]
  var peers: [Peer]
  /// Whether the pointer rests against an edge for a moment before it crosses.
  var pauseAtEdges: Bool?
  /// Whether the clipboard goes along with the pointer.
  var shareClipboard: Bool?
  var configPath: String
  var notices: [Notice]
  var platform: MacPlatform

  /// The rows the window shows as banners.
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
  /// What its row says; an unreachable computer's error.
  var detail: String
  /// Whether it may control this Mac.
  var allowControl: Bool
  /// How its keys act on this Mac.
  var keyboard: KeyboardMode
  /// Whether its scrolling is turned around on this Mac.
  var reverseScroll: Bool
  /// Its key's mark, as its tile showed it on the shelf.
  var mark: String
  var id: String { name }

  enum State: String, Decodable, Sendable {
    case paired, connecting, connected, unreachable
    case controllingThis = "controlling_this"
    case controlledFromHere = "controlled_from_here"
  }
}
/// src/core/keymap.rs KeyboardMode, in menu order.
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

  /// Unknown until zflow first looks for computers, which is when macOS asks.
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
  /// With `move_tile` and `preview`: every screen of the computer moves.
  var computer: Bool?
}
/// The engine answers each request with a snapshot, but `preview` with a
/// Preview, or null where the screens cannot go.
private struct CoreResponse<Answer: Decodable>: Decodable {
  var snapshot: Answer?
  var error: String?
}
struct AppError: LocalizedError {
  let message: String
  var errorDescription: String? { message }
}

// The actor serializes every request; only the lens calls skip it. The handle owns its Rust
// worker and joins it on release.
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
/// The engine for the lens calls, which are safe from any thread and never
/// wait, so the lens makes them straight from the main thread. It holds the
/// engine weakly: once the bridge lets go of it, every call does nothing.
final class LensHandle: Sendable {
  private weak let core: CoreHandle?
  fileprivate init(_ core: CoreHandle) { self.core = core }
  /// Runs `body` on the live engine, or returns nil once it is gone.
  func callAsFunction<T>(_ body: (OpaquePointer) -> T) -> T? {
    guard let core else { return nil }
    return withExtendedLifetime(core) { body(core.pointer) }
  }
}
actor CoreBridge {
  let path: String
  private var handle: CoreHandle?
  // A request queued behind shutdown must not start a new engine during quit.
  private var closed = false
  init(path: String) { self.path = path }
  func request(_ request: CoreRequest) throws -> Snapshot {
    guard let snapshot = try call(request, answer: Snapshot.self) else {
      throw AppError(message: "The sharing engine returned no status")
    }
    return snapshot
  }
  /// Where a dragged screen would land; nil where it cannot go.
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
  /// The engine, started on first use.
  private func open() throws -> CoreHandle {
    guard !closed else { throw AppError(message: "The sharing engine has stopped") }
    if let handle { return handle }
    let handle = try CoreHandle(path: path)
    self.handle = handle
    return handle
  }
  /// The engine for the lens, which calls it without this actor.
  func lens() throws -> LensHandle { LensHandle(try open()) }
  func shutdown() {
    closed = true
    handle = nil
  }
}
