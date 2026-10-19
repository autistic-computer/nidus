import Carbon

/// ⌃⌥L from any app, which opens or closes the lens. Carbon's hot keys need no
/// Accessibility permission, and macOS keeps the keystroke from the app in front.
@MainActor
final class LensHotKey {
  static let keyCode = UInt32(kVK_ANSI_L)
  static let modifiers = UInt32(controlKey | optionKey)
  /// How the menu shows it.
  static let label = "⌃⌥L"
  /// 'zflw', which tells this app's hot key from others in the event.
  nonisolated private static let signature: OSType = 0x7A66_6C77

  private let action: @MainActor () -> Void
  private var hotKey: EventHotKeyRef?
  private var handler: EventHandlerRef?

  init(action: @escaping @MainActor () -> Void) { self.action = action }

  /// Takes the key until `unregister`. The owner keeps this alive until then,
  /// since the handler finds it through an unretained pointer.
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
        // Carbon delivers hot keys on the main thread's event loop.
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
