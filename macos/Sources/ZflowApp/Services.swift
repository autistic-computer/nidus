import AppKit
import Foundation
import Observation
import Security
import ServiceManagement
import Synchronization
import XPC

@MainActor @Observable
final class Services {
  var helperReady = false
  var helperNote = "Checking the Wi-Fi helper…"
  var helperAction: String?
  var helperBusy = false
  var loginEnabled = SMAppService.mainApp.status == .enabled
  var error: String?
  let hasSigningTeam = Services.signingTeam()
  private var refreshing = false
  private let helper = SMAppService.daemon(plistName: "io.zflow.awdl.plist")

  private nonisolated static func signingTeam() -> Bool {
    var code: SecCode?
    var information: CFDictionary?
    var staticCode: SecStaticCode?
    guard SecCodeCopySelf([], &code) == errSecSuccess, let code,
      SecCodeCopyStaticCode(code, [], &staticCode) == errSecSuccess, let staticCode,
      SecCodeCopySigningInformation(
        staticCode, SecCSFlags(rawValue: kSecCSSigningInformation), &information) == errSecSuccess,
      let information = information as? [String: Any]
    else { return false }
    return information[kSecCodeInfoTeamIdentifier as String] as? String != nil
  }

  func refreshLogin() { loginEnabled = SMAppService.mainApp.status == .enabled }

  func refreshHelper() async {
    guard !refreshing else { return }
    refreshing = true
    defer { refreshing = false }
    guard hasSigningTeam else {
      helperReady = false
      helperNote = "Needs a signed build of zflow. Sharing works without it."
      helperAction = nil
      return
    }
    switch helper.status {
    case .notRegistered, .notFound:
      helperReady = false
      helperNote = "Needs a helper. macOS asks you to allow it."
      helperAction = "Install…"
    case .requiresApproval:
      helperReady = false
      helperNote = "Allow zflow in System Settings › General › Login Items & Extensions."
      helperAction = "Allow…"
    case .enabled:
      helperReady = await Self.checkHelper()
      helperNote = helperReady ? "" : "The helper is installed but did not answer."
      helperAction = helperReady ? nil : "Repair…"
    @unknown default:
      helperReady = false
      helperNote = "macOS returned an unknown status for the helper."
      helperAction = nil
    }
  }

  func installOrRepair() async {
    guard !helperBusy else { return }
    helperBusy = true
    defer { helperBusy = false }
    error = nil
    guard hasSigningTeam else {
      error = helperNote
      return
    }
    do {
      let status = helper.status
      if status == .requiresApproval {
        SMAppService.openSystemSettingsLoginItems()
      } else if status == .enabled, await Self.checkHelper() {
      } else {
        if helper.status == .enabled { try await helper.unregister() }
        try helper.register()
        if helper.status == .requiresApproval { SMAppService.openSystemSettingsLoginItems() }
      }
      await refreshHelper()
    } catch {
      if helper.status == .requiresApproval {
        SMAppService.openSystemSettingsLoginItems()
        await refreshHelper()
      } else {
        self.error = error.localizedDescription
      }
    }
  }

  func setLogin(_ enabled: Bool) async {
    error = nil
    do {
      if enabled && SMAppService.mainApp.status != .requiresApproval {
        try SMAppService.mainApp.register()
      } else if !enabled {
        try await SMAppService.mainApp.unregister()
      }
      loginEnabled = SMAppService.mainApp.status == .enabled
      if enabled && SMAppService.mainApp.status == .requiresApproval {
        SMAppService.openSystemSettingsLoginItems()
      }
    } catch {
      if enabled && SMAppService.mainApp.status == .requiresApproval {
        SMAppService.openSystemSettingsLoginItems()
      } else {
        self.error = error.localizedDescription
      }
      loginEnabled = SMAppService.mainApp.status == .enabled
    }
  }

  private nonisolated static func checkHelper() async -> Bool {
    guard
      let session = try? XPCSession(
        machService: "io.zflow.awdl", options: .privileged,
        requirement: .isFromSameTeam(andMatchesSigningIdentifier: "io.zflow.awdl-daemon"))
    else { return false }
    var request = XPCDictionary()
    request["command"] = "check"
    return await withCheckedContinuation { continuation in
      let answer = Answer(continuation)
      DispatchQueue.global().asyncAfter(deadline: .now() + 3) {
        session.cancel(reason: "Helper check timed out")
        answer.give(false)
      }
      session.send(message: request) { result in
        let reply = try? result.get()
        let ready: Bool? = reply?["ready"]
        session.cancel(reason: "Helper checked")
        answer.give(ready == true)
      }
    }
  }
}

private final class Answer: Sendable {
  private let continuation: Mutex<CheckedContinuation<Bool, Never>?>
  init(_ continuation: CheckedContinuation<Bool, Never>) {
    self.continuation = Mutex(continuation)
  }
  func give(_ ready: Bool) {
    continuation.withLock { $0.take() }?.resume(returning: ready)
  }
}
