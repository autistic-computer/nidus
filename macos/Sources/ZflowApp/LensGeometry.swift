import AppKit
import ZflowCore

/// The other computer's desktop fitted whole onto a screen of this Mac,
/// centred, with bars where the shapes differ. Points are the overlay's, y up
/// as AppKit has them; pixels are the desktop's, y down from its top left.
struct LensFit: Equatable {
  /// The desktop in its own pixels.
  let desktop: CGSize
  /// Where the desktop lands on the overlay.
  let frame: CGRect
  /// Points per desktop pixel.
  let scale: CGFloat

  /// Nil while either size is empty.
  init?(desktop: CGSize, in bounds: CGRect) {
    guard desktop.width > 0, desktop.height > 0, bounds.width > 0, bounds.height > 0 else {
      return nil
    }
    self.desktop = desktop
    scale = min(bounds.width / desktop.width, bounds.height / desktop.height)
    let size = CGSize(width: desktop.width * scale, height: desktop.height * scale)
    frame = CGRect(
      x: bounds.midX - size.width / 2, y: bounds.midY - size.height / 2,
      width: size.width, height: size.height)
  }

  func point(_ pixel: CGPoint) -> CGPoint {
    CGPoint(x: frame.minX + pixel.x * scale, y: frame.maxY - pixel.y * scale)
  }
  /// Where `point` falls on the desktop, which may be off it, in the bars.
  func pixel(_ point: CGPoint) -> CGPoint {
    CGPoint(x: (point.x - frame.minX) / scale, y: (frame.maxY - point.y) / scale)
  }
  /// The nearest pixel on the desktop.
  func clamped(_ pixel: CGPoint) -> CGPoint {
    CGPoint(
      x: min(max(pixel.x, 0), desktop.width - 1), y: min(max(pixel.y, 0), desktop.height - 1))
  }
  /// A length on the overlay in desktop pixels.
  func pixels(_ points: CGFloat) -> CGFloat { points / scale }
}

/// The desktop cut into the squares the engine hands out, numbered row by row
/// from the top left.
struct LensGrid: Equatable {
  static let chunk = Int(ZFLOW_LENS_CHUNK)
  let width: Int
  let height: Int

  var columns: Int { (width + Self.chunk - 1) / Self.chunk }
  var rows: Int { (height + Self.chunk - 1) / Self.chunk }
  var count: Int { columns * rows }

  /// Chunk `id` in desktop pixels. The last column and row are smaller.
  func rect(_ id: Int) -> CGRect? {
    guard id >= 0, id < count else { return nil }
    let x = id % columns * Self.chunk
    let y = id / columns * Self.chunk
    return CGRect(
      x: x, y: y, width: min(Self.chunk, width - x), height: min(Self.chunk, height - y))
  }
  /// Chunk `id` in a layer of the desktop's size with y up, as Core Animation
  /// lays out sublayers on the Mac.
  func frame(_ id: Int) -> CGRect? {
    rect(id).map {
      CGRect(x: $0.minX, y: CGFloat(height) - $0.maxY, width: $0.width, height: $0.height)
    }
  }
  /// The chunks the engine looks at for a circle: every one its bounding
  /// square touches, as src/lens/viewer.rs picks them.
  func chunks(around center: CGPoint, radius: CGFloat) -> [Int] {
    guard center.x.isFinite, center.y.isFinite, radius.isFinite, width > 0, height > 0 else {
      return []
    }
    func span(_ middle: CGFloat, _ size: Int) -> [Int] {
      let low = Int(min(max(middle - radius, 0), CGFloat(Int32.max))) / Self.chunk
      let high = min(Int(min(max(middle + radius, 0), CGFloat(Int32.max))), size - 1) / Self.chunk
      return low <= high ? Array(low...high) : []
    }
    return span(center.y, height).flatMap { row in
      span(center.x, width).map { row * columns + $0 }
    }
  }
}

/// How the Mac's input reads on the other computer.
enum LensInput {
  /// Points a line of an old-style scroll wheel moves.
  static let line: CGFloat = 16
  static let escape: UInt16 = 53
  static let capsLock: UInt16 = 57

  /// The engine's button for AppKit's `buttonNumber`: 1 left, 2 middle,
  /// 3 right, 4 back, 5 forward.
  static func button(_ number: Int) -> UInt32? {
    switch number {
    case 0: 1
    case 1: 3
    case 2: 2
    case 3: 4
    case 4: 5
    default: nil
    }
  }

  /// A scroll in pixels for the engine, where positive moves toward the end
  /// of the page. AppKit's deltas point the way the content moves and already
  /// follow the natural scrolling setting, so the content moves the same way
  /// it would on this Mac.
  static func scroll(dx: CGFloat, dy: CGFloat, precise: Bool) -> (dx: Double, dy: Double) {
    let unit = precise ? 1 : line
    return (-dx * unit, -dy * unit)
  }

  enum KeyDown: Equatable { case close, copy, send }
  /// Escape closes the lens and ⌘C copies the other computer's selection
  /// here; every other key goes over. `characters` is the event's
  /// `charactersIgnoringModifiers`, so ⌘C follows the keyboard layout.
  static func keyDown(_ keyCode: UInt16, characters: String?, flags: NSEvent.ModifierFlags)
    -> KeyDown
  {
    if keyCode == escape { return .close }
    let held = flags.intersection([.command, .control, .option, .shift])
    if held == .command, characters?.lowercased() == "c" { return .copy }
    return .send
  }

  /// Whether modifier `keyCode` is down after a flagsChanged event, from the
  /// left and right bits AppKit keeps in the raw flags, so letting go of one
  /// Shift while the other is held reads right. Nil for keys that are not
  /// modifiers, and for Caps Lock, which toggles.
  static func modifier(_ keyCode: UInt16, flags: UInt) -> Bool? {
    // This key's bit, both sides' bits, and the flag for either side.
    let (side, sides, family): (UInt, UInt, NSEvent.ModifierFlags)
    switch keyCode {
    case 55: (side, sides, family) = (0x08, 0x18, .command)
    case 54: (side, sides, family) = (0x10, 0x18, .command)
    case 56: (side, sides, family) = (0x02, 0x06, .shift)
    case 60: (side, sides, family) = (0x04, 0x06, .shift)
    case 58: (side, sides, family) = (0x20, 0x60, .option)
    case 61: (side, sides, family) = (0x40, 0x60, .option)
    case 59: (side, sides, family) = (0x01, 0x2001, .control)
    case 62: (side, sides, family) = (0x2000, 0x2001, .control)
    case 63: return flags & NSEvent.ModifierFlags.function.rawValue != 0
    default: return nil
    }
    // Events made by software may carry only the flag for either side.
    if flags & sides == 0 { return flags & family.rawValue != 0 }
    return flags & side != 0
  }
}
