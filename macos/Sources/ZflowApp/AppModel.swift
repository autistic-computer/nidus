import AppKit
import Foundation
import Observation
import SwiftUI

enum Page: Hashable {
  case computers
  case settings
}

enum Owner: Hashable, Sendable {
  case thisMac
  case peer(String)

  init(_ peer: String?) { self = peer.map(Owner.peer) ?? .thisMac }
  var peer: String? {
    if case .peer(let name) = self { name } else { nil }
  }
}

@MainActor @Observable
final class AppModel {
  var snapshot: Snapshot?
  var error: String?
  var failure: String?
  var page = Page.computers
  var selection: Owner?
  var adjusting: Owner?
  var addingAddress = false
  @ObservationIgnored var joined: ((Notice) -> Void)?
  @ObservationIgnored private var lastNotice: UInt64 = 0
  var askToMove = AppLocation.main.needsMove
  var pasteAccess = NSPasteboard.general.accessBehavior
  var busy = false
  let services = Services()
  let updates = Updates()
  let lens = LensController()
  private(set) var lastLooked: String?
  @ObservationIgnored private var hotKey: LensHotKey?
  let configPath: String
  let showSettingsAtLaunch: Bool
  @ObservationIgnored var openWindow: OpenWindowAction?
  private let core: CoreBridge
  private var poller: Task<Void, Never>?
  @ObservationIgnored private var activity: (any NSObjectProtocol)?
  @ObservationIgnored private var wake: (any NSObjectProtocol)?
  @ObservationIgnored private var failureTimer: Task<Void, Never>?

  init(arguments: [String] = ProcessInfo.processInfo.arguments, polling: Bool = true) {
    if let index = arguments.firstIndex(of: "--config"), arguments.indices.contains(index + 1) {
      configPath = URL(fileURLWithPath: arguments[index + 1]).standardizedFileURL.path
    } else {
      configPath =
        FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(
          "Library/Application Support/zflow/zflow.toml"
        ).path
    }
    showSettingsAtLaunch = arguments.contains("--show-settings")
    core = CoreBridge(path: configPath)
    guard polling else { return }
    hotKey = LensHotKey { [weak self] in self?.toggleLens() }
    hotKey?.register()
    poller = Task { [weak self] in
      var ticks = 0
      while !Task.isCancelled {
        guard let self else { return }
        await self.perform(CoreRequest(command: "snapshot"), quiet: true)
        let access = NSPasteboard.general.accessBehavior
        if access != self.pasteAccess { self.pasteAccess = access }
        if ticks % 20 == 0 { Task { await self.checkServices() } }
        ticks += 1
        try? await Task.sleep(for: .milliseconds(500))
      }
    }
    wake = NSWorkspace.shared.notificationCenter.addObserver(
      forName: NSWorkspace.didWakeNotification, object: nil, queue: .main
    ) { [weak self] _ in
      MainActor.assumeIsolated { self?.send(CoreRequest(command: "retry")) }
    }
  }

  var title: String { snapshot?.status.title ?? "Needs attention" }
  var healthy: Bool {
    snapshot.map { [.ready, .controlling, .controlled].contains($0.status.state) } ?? false
  }

  func send(_ request: CoreRequest) {
    Task { await perform(request) }
  }
  func perform(_ request: CoreRequest, quiet: Bool = false) async {
    guard let message = await attempt(request) else { return }
    if snapshot == nil {
      error = message
    } else if !quiet {
      report(message)
    }
  }
  func attempt(_ request: CoreRequest) async -> String? {
    guard !busy else { return nil }
    do {
      apply(try await core.request(request))
      error = nil
      return nil
    } catch {
      return error.localizedDescription
    }
  }

  func apply(_ next: Snapshot) {
    if next != snapshot { snapshot = next }
    for notice in next.notices where notice.id > lastNotice {
      lastNotice = notice.id
      joined?(notice)
    }
    holdActivity(next.sharing == true)
    let gone = { (owner: Owner?) in
      owner?.peer.map { name in !next.peers.contains { $0.name == name } } ?? false
    }
    if gone(selection) { selection = nil }
    if gone(adjusting) { adjusting = nil }
  }

  func move(_ move: Move) async {
    await perform(move.request)
  }
  func preview(_ move: Move) async -> Preview? {
    guard !busy else { return nil }
    return try? await core.preview(move.preview)
  }

  func report(_ message: String?) {
    guard let message else { return }
    failure = message
    failureTimer?.cancel()
    failureTimer = Task { [weak self] in
      try? await Task.sleep(for: .seconds(8))
      if !Task.isCancelled { self?.failure = nil }
    }
  }

  private func holdActivity(_ sharing: Bool) {
    if sharing, activity == nil {
      activity = ProcessInfo.processInfo.beginActivity(
        options: [.userInitiatedAllowingIdleSystemSleep, .latencyCritical],
        reason: "Sharing input with another computer")
    } else if !sharing, let activity {
      ProcessInfo.processInfo.endActivity(activity)
      self.activity = nil
    }
  }

  func look(at name: String) {
    guard !busy else { return }
    lastLooked = name
    Task {
      let handle: LensHandle
      do { handle = try await core.lens() } catch { return report(error.localizedDescription) }
      let session = lens.show(name, handle: handle)
      let failure = await attempt(CoreRequest(command: "lens", name: name))
      lens.requested(session, failure: failure)
    }
  }

  func toggleLens() {
    if lens.isOpen { return lens.close() }
    guard let name = Self.lensTarget(peers: snapshot?.peers ?? [], last: lastLooked) else {
      return NSSound.beep()
    }
    look(at: name)
  }
  nonisolated static func lensTarget(peers: [Peer], last: String?) -> String? {
    let linked = peers.filter(\.state.linked)
    if let last, linked.contains(where: { $0.name == last }) { return last }
    return linked.first?.name
  }

  func show(computer name: String) {
    selection = .peer(name)
    show(.computers)
  }

  func show(_ page: Page? = nil) {
    if let page { self.page = page }
    guard let openWindow else { return }
    NSApplication.shared.setActivationPolicy(.regular)
    openWindow(id: "main")
    NSApplication.shared.activate()
  }

  nonisolated static let hostName: String = {
    var buffer = [CChar](repeating: 0, count: 256)
    guard gethostname(&buffer, buffer.count) == 0 else { return "this Mac" }
    var name = buffer.withUnsafeBufferPointer { String(cString: $0.baseAddress!) }
    if name.lowercased().hasSuffix(".local") { name.removeLast(".local".count) }
    return name.isEmpty ? "this Mac" : name
  }()

  func fix(for row: Health) -> HealthAction? {
    if let action = row.action { return action }
    if row.id == "clipboard", pasteAccess != .alwaysAllow {
      return HealthAction(label: "Allow…", command: "allow_paste")
    }
    return nil
  }
  func fix(_ action: HealthAction) {
    switch action.command {
    case "allow_accessibility": openAccessibility()
    case "allow_paste": allowPaste()
    case "open_config": openConfig()
    default: send(CoreRequest(command: action.command))
    }
  }
  func openConfig() { NSWorkspace.shared.open(URL(fileURLWithPath: configPath)) }
  func openAccessibility() {
    send(CoreRequest(command: "allow_accessibility"))
    openPrivacy("Privacy_Accessibility")
  }
  func allowLocalNetwork() {
    if snapshot?.platform.localNetwork == .unknown {
      send(CoreRequest(command: "discover"))
    } else {
      openPrivacy("Privacy_LocalNetwork")
    }
  }
  func allowPaste() {
    if pasteAccess == .default {
      _ = NSPasteboard.general.string(forType: .string)
      pasteAccess = NSPasteboard.general.accessBehavior
    } else {
      openPrivacy("Privacy_Pasteboard")
    }
  }
  private func openPrivacy(_ anchor: String) {
    NSWorkspace.shared.open(
      URL(string: "x-apple.systempreferences:com.apple.preference.security?\(anchor)")!)
  }

  func watchAccessibility() async {
    guard snapshot?.platform.accessibility == false else { return }
    while !Task.isCancelled && snapshot?.platform.accessibility == false {
      await perform(CoreRequest(command: "check_accessibility"), quiet: true)
      try? await Task.sleep(for: .milliseconds(500))
    }
    if snapshot?.platform.accessibility == true { NSApplication.shared.activate() }
  }

  func checkServices() async {
    services.refreshLogin()
    guard snapshot?.platform.blockAwdl == true else { return }
    await services.refreshHelper()
    await perform(
      CoreRequest(command: "helper_ready", ready: services.helperReady), quiet: true)
  }
  func setAwdl(_ enabled: Bool) {
    Task {
      await perform(CoreRequest(command: "set_awdl", enabled: enabled))
      if enabled { await checkServices() }
    }
  }
  func fixHelper() {
    Task {
      await services.installOrRepair()
      report(services.error)
      await perform(CoreRequest(command: "helper_ready", ready: services.helperReady))
    }
  }
  func setLogin(_ enabled: Bool) {
    Task {
      await services.setLogin(enabled)
      report(services.error)
    }
  }

  func quit() { NSApplication.shared.terminate(nil) }
  func shutdown() async {
    guard !busy else { return }
    busy = true
    poller?.cancel()
    hotKey?.unregister()
    lens.close()
    await core.shutdown()
  }
}

extension Peer.State {
  var linked: Bool { [.connected, .controllingThis, .controlledFromHere].contains(self) }
}
