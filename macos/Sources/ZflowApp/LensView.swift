import AppKit
import QuartzCore

/// The borderless panel the lens draws in, over everything on one screen. It
/// takes keys without bringing zflow to the front, so the app the person was
/// in keeps its place.
final class LensPanel: NSPanel {
  init(screen: NSScreen) {
    super.init(
      contentRect: screen.frame, styleMask: [.borderless, .nonactivatingPanel],
      backing: .buffered, defer: false)
    isOpaque = false
    backgroundColor = .clear
    hasShadow = false
    // Above the menu bar and the Dock.
    level = .screenSaver
    collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
    hidesOnDeactivate = false
    isReleasedWhenClosed = false
    animationBehavior = .none
    acceptsMouseMovedEvents = true
  }
  override var canBecomeKey: Bool { true }
  override var canBecomeMain: Bool { false }
}

/// Draws the lens with Core Animation and hands the pointer to the controller.
/// The desktop layer holds the other computer's desktop in its own pixels,
/// one sublayer per chunk, scaled to fit the screen; a circle around the
/// pointer masks it, over a dark fill for chunks not there yet.
final class LensView: NSView {
  weak var controller: LensController?
  let radius: CGFloat
  private let clip = CALayer()
  private let circle = CALayer()
  private let desktop = CALayer()
  private let ring = CALayer()
  private let badge = CALayer()
  private let label = CATextLayer()
  private var chunks: [Int: CALayer] = [:]
  private var grid: LensGrid?

  init(frame: CGRect, radius: CGFloat, backingScale: CGFloat) {
    self.radius = radius
    super.init(frame: frame)
    // Hosts its own layers, so AppKit leaves their geometry alone: y is up.
    let root = CALayer()
    layer = root
    wantsLayer = true
    let disc = CGRect(x: 0, y: 0, width: radius * 2, height: radius * 2)
    circle.bounds = disc
    circle.cornerRadius = radius
    circle.backgroundColor = .black
    clip.frame = bounds
    clip.backgroundColor = CGColor(gray: 0.08, alpha: 1)
    clip.mask = circle
    desktop.anchorPoint = .zero
    clip.addSublayer(desktop)
    ring.bounds = disc
    ring.cornerRadius = radius
    ring.borderWidth = 1.5
    ring.borderColor = CGColor(gray: 1, alpha: 0.85)
    badge.backgroundColor = CGColor(gray: 0, alpha: 0.7)
    badge.cornerRadius = 6
    badge.isHidden = true
    label.contentsScale = backingScale
    label.isWrapped = true
    label.alignmentMode = .center
    label.truncationMode = .end
    badge.addSublayer(label)
    ring.addSublayer(badge)
    root.addSublayer(clip)
    root.addSublayer(ring)
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

  /// The lens centre: where the pointer was last seen.
  var center: CGPoint { circle.position }
  func contains(_ point: CGPoint) -> Bool {
    hypot(point.x - center.x, point.y - center.y) <= radius
  }
  func moveCircle(to point: CGPoint) {
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    circle.position = point
    ring.position = point
    CATransaction.commit()
  }

  /// Lays the desktop out for `fit`, dropping the chunks of an earlier size.
  func fit(_ fit: LensFit, grid: LensGrid) {
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    for layer in chunks.values { layer.removeFromSuperlayer() }
    chunks = [:]
    self.grid = grid
    desktop.bounds = CGRect(origin: .zero, size: fit.desktop)
    desktop.position = fit.frame.origin
    desktop.transform = CATransform3DMakeScale(fit.scale, fit.scale, 1)
    CATransaction.commit()
  }
  /// Shows chunk `id`'s pixels. The caller wraps a frame's chunks in one
  /// transaction with actions off, so nothing fades in.
  func draw(_ image: CGImage, chunk id: Int) {
    if let layer = chunks[id] {
      layer.contents = image
      return
    }
    guard let frame = grid?.frame(id) else { return }
    let layer = CALayer()
    layer.frame = frame
    layer.contents = image
    layer.contentsGravity = .resize
    // One image pixel per desktop pixel; the desktop layer does the scaling.
    layer.contentsScale = 1
    layer.magnificationFilter = .linear
    layer.minificationFilter = .linear
    layer.isOpaque = true
    layer.edgeAntialiasingMask = []
    chunks[id] = layer
    desktop.addSublayer(layer)
  }

  /// A line under the pointer, inside the circle; nil hides it.
  func show(_ message: String?) {
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    defer { CATransaction.commit() }
    guard let message else {
      badge.isHidden = true
      return
    }
    let paragraph = NSMutableParagraphStyle()
    paragraph.alignment = .center
    let text = NSAttributedString(
      string: message,
      attributes: [
        .font: NSFont.systemFont(ofSize: 13, weight: .medium),
        .foregroundColor: NSColor.white, .paragraphStyle: paragraph,
      ])
    let width = radius * 1.5
    let size = text.boundingRect(
      with: CGSize(width: width, height: radius), options: [.usesLineFragmentOrigin]
    ).size
    label.string = text
    label.frame = CGRect(x: 10, y: 5, width: width, height: ceil(size.height))
    badge.bounds = CGRect(x: 0, y: 0, width: width + 20, height: ceil(size.height) + 10)
    badge.position = CGPoint(x: radius, y: radius - 48)
    badge.isHidden = false
  }

  override var acceptsFirstResponder: Bool { true }
  override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
  // The pointer is followed even while zflow is not the active app.
  override func updateTrackingAreas() {
    super.updateTrackingAreas()
    guard trackingAreas.isEmpty else { return }
    addTrackingArea(
      NSTrackingArea(
        rect: .zero, options: [.mouseMoved, .activeAlways, .inVisibleRect], owner: self))
  }
  override func mouseMoved(with event: NSEvent) { controller?.move(event) }
  override func mouseDragged(with event: NSEvent) { controller?.move(event) }
  override func rightMouseDragged(with event: NSEvent) { controller?.move(event) }
  override func otherMouseDragged(with event: NSEvent) { controller?.move(event) }
  override func mouseDown(with event: NSEvent) { controller?.press(event, pressed: true) }
  override func mouseUp(with event: NSEvent) { controller?.press(event, pressed: false) }
  override func rightMouseDown(with event: NSEvent) { controller?.press(event, pressed: true) }
  override func rightMouseUp(with event: NSEvent) { controller?.press(event, pressed: false) }
  override func otherMouseDown(with event: NSEvent) { controller?.press(event, pressed: true) }
  override func otherMouseUp(with event: NSEvent) { controller?.press(event, pressed: false) }
  override func scrollWheel(with event: NSEvent) { controller?.scroll(event) }
}
