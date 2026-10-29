import Carbon

@MainActor
final class LensHotKey {
  static let keyCode = UInt32(kVK_ANSI_L)
  static let modifiers = UInt32(controlKey | optionKey)
  static let label = "⌃⌥L"
  nonisolated private static let signature: OSType = 0x7A66_6C77

  private let action: @MainActor () -> Void
  private var hotKey: EventHotKeyRef?
  private var handler: EventHandlerRef?

  init(action: @escaping @MainActor () -> Void) { self.action = action }

  func register() {
    guard hotKey == nil else { return }
    var pressed = EventTypeSpec(
      eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
    let status = InstallEventHandler(
      GetApplicationEventTarget(),
      { _, event, context in
        var id = EventHotKeyID()
        let read = GetEventParameter(
          event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID), nil,
          MemoryLayout<EventHotKeyID>.size, nil, &id)
        guard read == noErr, id.signature == LensHotKey.signature, let context else {
          return OSStatus(eventNotHandledErr)
        }
        let hotKey = Unmanaged<LensHotKey>.fromOpaque(context)
        MainActor.assumeIsolated { hotKey.takeUnretainedValue().action() }
        return noErr
      }, 1, &pressed, Unmanaged.passUnretained(self).toOpaque(), &handler)
    guard status == noErr else { return }
    RegisterEventHotKey(
      Self.keyCode, Self.modifiers, EventHotKeyID(signature: Self.signature, id: 1),
      GetApplicationEventTarget(), 0, &hotKey)
  }

  func unregister() {
    if let hotKey { UnregisterEventHotKey(hotKey) }
    if let handler { RemoveEventHandler(handler) }
    hotKey = nil
    handler = nil
  }
}
