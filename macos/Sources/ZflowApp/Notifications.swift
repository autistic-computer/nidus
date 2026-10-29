import UserNotifications

@MainActor
final class Notifier: NSObject, UNUserNotificationCenterDelegate {
  nonisolated static let category = "joined"
  nonisolated static let forget = "forget"
  nonisolated static let update = "update"
  private weak var model: AppModel?

  func listen() {
    let center = UNUserNotificationCenter.current()
    center.delegate = self
    let forget = UNNotificationAction(
      identifier: Self.forget, title: "Forget", options: [.destructive])
    center.setNotificationCategories([
      UNNotificationCategory(identifier: Self.category, actions: [forget], intentIdentifiers: [])
    ])
  }

  func attach(_ model: AppModel) {
    self.model = model
    model.joined = { [weak self] notice in self?.post(notice) }
    model.updates.found = { [weak self] version in self?.post(update: version) }
  }

  private func post(update version: String) {
    let content = UNMutableNotificationContent()
    content.title = "zflow \(version) is available"
    content.body = "Click to install it."
    deliver(UNNotificationRequest(identifier: Self.update, content: content, trigger: nil))
  }

  private func post(_ notice: Notice) {
    let content = UNMutableNotificationContent()
    content.title = "\(notice.name) joined"
    content.body = "Its mark is \(notice.mark). It can share this Mac’s keyboard and mouse. Not yours?"
    content.categoryIdentifier = Self.category
    content.userInfo = ["name": notice.name]
    deliver(
      UNNotificationRequest(identifier: "joined-\(notice.id)", content: content, trigger: nil))
  }

  private func deliver(_ request: UNNotificationRequest) {
    Task {
      let center = UNUserNotificationCenter.current()
      guard (try? await center.requestAuthorization(options: [.alert, .sound])) == true else {
        return
      }
      try? await center.add(request)
    }
  }

  nonisolated func userNotificationCenter(
    _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse
  ) async {
    if response.notification.request.identifier == Self.update {
      await MainActor.run { model?.updates.check() }
      return
    }
    guard let name = response.notification.request.content.userInfo["name"] as? String else {
      return
    }
    let forget = response.actionIdentifier == Self.forget
    await MainActor.run {
      guard let model else { return }
      if forget {
        model.send(CoreRequest(command: "forget", name: name))
      } else {
        model.show(computer: name)
      }
    }
  }

  nonisolated func userNotificationCenter(
    _ center: UNUserNotificationCenter, willPresent notification: UNNotification
  ) async -> UNNotificationPresentationOptions {
    [.banner, .list]
  }
}
