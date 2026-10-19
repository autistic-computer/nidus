import AppKit
import QuartzCore
import ZflowCore

/// The lens: another computer's screen in a circle around the pointer, which
/// sends the pointer, clicks, scrolling and keys to that computer while it is
/// open. Moving the pointer only moves the circle. Pixels come in on the
/// display's ticks, and only for the chunks under the circle that changed.
@MainActor
final class LensController: NSObject {
  /// The circle's radius on this screen, in points.
  static let radius: CGFloat = 200
  private static let space = CGColorSpace(name: CGColorSpace.sRGB)!
  /// BGRx in memory: little-endian words with the unused byte first.
  private static let bgrx = CGBitmapInfo(
    rawValue: CGImageAlphaInfo.noneSkipFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)
  private static let stride = LensGrid.chunk * 4

  private var handle: LensHandle?
  private var panel: LensPanel?
  private var view: LensView?
  private var name = ""
  /// Grows with each lens shown, so a late answer to an earlier one is told apart.
  private var session = 0
  /// When the engine took the request; until then its status is the last lens's.
  private var requested: ContinuousClock.Instant?
  /// Whether the engine has said it is opening or open.
  private var opened = false
  /// Shows why the lens failed, then closes it.
  private var closing: Task<Void, Never>?
  private var link: CADisplayLink?
  private var monitor: Any?
  private var observers: [any NSObjectProtocol] = []

  private var fit: LensFit?
  private var grid: LensGrid?
  /// The pointer on the overlay, which is the circle's centre.
  private var pointer = CGPoint.zero
  /// The frame count and circle the chunks were last asked for at.
  private var polled: (frames: UInt64, center: CGPoint)?
  private var ids: [UInt32] = []
  private var buffer = [UInt8](repeating: 0, count: LensGrid.chunk * stride)
  /// What is held down on the other computer, let go when the lens closes.
  private var keys: Set<UInt16> = []
  private var buttons: Set<UInt32> = []
  /// Keys whose press was not sent, so neither is their release.
  private var swallowed: Set<UInt16> = []

  var isOpen: Bool { panel != nil }

  /// Shows the lens around the pointer while the engine opens it onto `name`,
  /// and returns the session that `requested` answers.
  func show(_ name: String, handle: LensHandle) -> Int {
    close()
    session += 1
    self.name = name
    self.handle = handle
    let mouse = NSEvent.mouseLocation
    guard
      let screen = NSScreen.screens.first(where: { NSMouseInRect(mouse, $0.frame, false) })
        ?? NSScreen.main
    else { return session }
    let panel = LensPanel(screen: screen)
    let view = LensView(
      frame: CGRect(origin: .zero, size: screen.frame.size), radius: Self.radius,
      backingScale: screen.backingScaleFactor)
    view.controller = self
    panel.contentView = view
    self.panel = panel
    self.view = view
    pointer = view.convert(panel.convertPoint(fromScreen: mouse), from: nil)
    view.moveCircle(to: pointer)
    view.show("Opening \(name)…")
    panel.makeKeyAndOrderFront(nil)
    panel.makeFirstResponder(view)

    let link = view.displayLink(target: self, selector: #selector(tick))
    link.add(to: .main, forMode: .common)
    self.link = link
    // A local monitor sees the key-up of ⌘ shortcuts, which AppKit does not
    // pass to views, and keeps them from zflow's own menu.
    monitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .keyUp, .flagsChanged]) {
      [weak self] event in
      MainActor.assumeIsolated { self?.key(event) == true } ? nil : event
    }
    let center = NotificationCenter.default
    let leave: @Sendable (Notification) -> Void = { [weak self] _ in
      MainActor.assumeIsolated { self?.close() }
    }
    observers = [
      center.addObserver(
        forName: NSWindow.didResignKeyNotification, object: panel, queue: .main, using: leave),
      center.addObserver(
        forName: NSApplication.didResignActiveNotification, object: nil, queue: .main,
        using: leave),
    ]
    return session
  }

  /// The engine has answered the request to open `session`.
  func requested(_ session: Int, failure: String?) {
    guard session == self.session, isOpen else {
      // Closed while the engine was opening it, and nothing else is open.
      if failure == nil, !isOpen { handle? { zflow_lens_close($0) } }
      return
    }
    if let failure {
      fail(failure)
    } else {
      requested = .now
    }
  }

  func close() {
    guard let panel else { return }
    closing?.cancel()
    closing = nil
    link?.invalidate()
    link = nil
    if let monitor { NSEvent.removeMonitor(monitor) }
    monitor = nil
    observers.forEach(NotificationCenter.default.removeObserver)
    observers = []
    let (keys, buttons) = (keys, buttons)
    handle? { app in
      for key in keys { zflow_lens_key(app, key, false) }
      for button in buttons { zflow_lens_button(app, button, false) }
      zflow_lens_close(app)
    }
    self.keys = []
    self.buttons = []
    swallowed = []
    panel.orderOut(nil)
    self.panel = nil
    view = nil
    fit = nil
    grid = nil
    polled = nil
    requested = nil
    opened = false
  }

  /// Shows `message` in the circle for a moment, then closes.
  private func fail(_ message: String) {
    guard closing == nil else { return }
    view?.show(message)
    closing = Task { [weak self] in
      try? await Task.sleep(for: .seconds(2))
      if !Task.isCancelled { self?.close() }
    }
  }

  @objc private func tick(_ link: CADisplayLink) {
    guard let requested, closing == nil else { return }
    guard let status = handle?({ zflow_lens_status($0) }) else { return close() }
    switch status.state {
    case 1:
      opened = true
    case 2:
      opened = true
      pull(status)
    case 3:
      let error = handle? { app -> String? in
        guard let text = zflow_lens_error(app) else { return nil }
        defer { zflow_string_free(text) }
        return String(cString: text)
      }
      fail((error ?? nil) ?? "The lens onto \(name) closed")
    default:
      if opened {
        close()
      } else if ContinuousClock.now - requested > .seconds(5) {
        fail("\(name) did not answer")
      }
    }
  }

  /// Copies the chunks under the circle that changed, and fits the desktop
  /// to the screen when its size is new.
  private func pull(_ status: ZflowLensStatus) {
    guard let view, let handle else { return }
    let size = CGSize(width: Int(status.width), height: Int(status.height))
    if fit?.desktop != size {
      guard let fit = LensFit(desktop: size, in: view.bounds) else { return }
      let grid = LensGrid(width: Int(status.width), height: Int(status.height))
      self.fit = fit
      self.grid = grid
      view.fit(fit, grid: grid)
      view.show(nil)
      ids = [UInt32](repeating: 0, count: grid.count)
      polled = nil
      sendMove()
    }
    guard let fit else { return }
    let center = fit.pixel(pointer)
    if let polled, polled.frames == status.frames, polled.center == center { return }
    polled = (status.frames, center)
    let radius = Double(fit.pixels(Self.radius))
    let stride = Self.stride
    handle { app in
      let count = ids.withUnsafeMutableBufferPointer {
        zflow_lens_dirty(app, center.x, center.y, radius, $0.baseAddress, $0.count)
      }
      guard count > 0 else { return }
      CATransaction.begin()
      CATransaction.setDisableActions(true)
      for id in ids[..<count] {
        var width: UInt32 = 0
        var height: UInt32 = 0
        let copied = buffer.withUnsafeMutableBufferPointer {
          zflow_lens_chunk(app, id, $0.baseAddress, $0.count, stride, &width, &height)
        }
        guard copied, width > 0, height > 0,
          let data = buffer.withUnsafeBufferPointer({
            CFDataCreate(nil, $0.baseAddress, stride * Int(height))
          }),
          let provider = CGDataProvider(data: data),
          let image = CGImage(
            width: Int(width), height: Int(height), bitsPerComponent: 8, bitsPerPixel: 32,
            bytesPerRow: stride, space: Self.space, bitmapInfo: Self.bgrx, provider: provider,
            decode: nil, shouldInterpolate: true, intent: .defaultIntent)
        else { continue }
        view.draw(image, chunk: Int(id))
      }
      CATransaction.commit()
    }
  }

  // MARK: Input

  /// Sends the pointer's place on the other computer, held to its desktop.
  private func sendMove() {
    guard let fit, closing == nil else { return }
    let pixel = fit.clamped(fit.pixel(pointer))
    let radius = Double(fit.pixels(Self.radius))
    handle? { zflow_lens_move($0, pixel.x, pixel.y, radius) }
  }

  func move(_ event: NSEvent) {
    guard let view else { return }
    let point = view.convert(event.locationInWindow, from: nil)
    // The tracking area and the window can both report the same move.
    guard point != pointer else { return }
    pointer = point
    view.moveCircle(to: point)
    sendMove()
  }

  func press(_ event: NSEvent, pressed: Bool) {
    guard let view else { return }
    let point = view.convert(event.locationInWindow, from: nil)
    // A click outside the circle, or one while it shows why it failed, closes it.
    if pressed, event.type == .leftMouseDown, !view.contains(point) || closing != nil {
      return close()
    }
    guard fit != nil, closing == nil, let button = LensInput.button(event.buttonNumber) else {
      return
    }
    if point != pointer {
      pointer = point
      view.moveCircle(to: point)
      sendMove()
    }
    if pressed {
      buttons.insert(button)
    } else if buttons.remove(button) == nil {
      return
    }
    handle? { zflow_lens_button($0, button, pressed) }
  }

  func scroll(_ event: NSEvent) {
    guard fit != nil, closing == nil else { return }
    let delta = LensInput.scroll(
      dx: event.scrollingDeltaX, dy: event.scrollingDeltaY,
      precise: event.hasPreciseScrollingDeltas)
    guard delta.dx != 0 || delta.dy != 0 else { return }
    handle? { zflow_lens_scroll($0, delta.dx, delta.dy) }
  }

  /// Takes the panel's key events; true when the lens used them up.
  private func key(_ event: NSEvent) -> Bool {
    guard let panel, event.window === panel else { return false }
    let code = event.keyCode
    switch event.type {
    case .keyDown:
      switch LensInput.keyDown(
        code, characters: event.charactersIgnoringModifiers, flags: event.modifierFlags)
      {
      case .close:
        close()
      case .copy:
        swallowed.insert(code)
        if closing == nil { handle? { zflow_lens_copy($0) } }
      case .send:
        // Repeats go over too, as the other computer's own keys would.
        send(code, pressed: true)
      }
    case .keyUp:
      if swallowed.remove(code) == nil { send(code, pressed: false) }
    case .flagsChanged:
      if code == LensInput.capsLock {
        // Each change is one press of the key, which toggles it there too.
        send(code, pressed: true)
        send(code, pressed: false)
      } else if let pressed = LensInput.modifier(code, flags: event.modifierFlags.rawValue),
        pressed != keys.contains(code)
      {
        send(code, pressed: pressed)
      }
    default:
      return false
    }
    return true
  }

  private func send(_ code: UInt16, pressed: Bool) {
    guard closing == nil else { return }
    if pressed {
      keys.insert(code)
    } else if keys.remove(code) == nil {
      return
    }
    handle? { zflow_lens_key($0, code, pressed) }
  }
}
