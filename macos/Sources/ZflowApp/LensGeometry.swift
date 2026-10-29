import AppKit
import ZflowCore

struct LensFit: Equatable {
  let desktop: CGSize
  let frame: CGRect
  let scale: CGFloat

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
  func pixel(_ point: CGPoint) -> CGPoint {
    CGPoint(x: (point.x - frame.minX) / scale, y: (frame.maxY - point.y) / scale)
  }
  func clamped(_ pixel: CGPoint) -> CGPoint {
    CGPoint(
      x: min(max(pixel.x, 0), desktop.width - 1), y: min(max(pixel.y, 0), desktop.height - 1))
  }
  func pixels(_ points: CGFloat) -> CGFloat { points / scale }
}

struct LensGrid: Equatable {
  static let chunk = Int(ZFLOW_LENS_CHUNK)
  let width: Int
  let height: Int

  var columns: Int { (width + Self.chunk - 1) / Self.chunk }
  var rows: Int { (height + Self.chunk - 1) / Self.chunk }
  var count: Int { columns * rows }

  func rect(_ id: Int) -> CGRect? {
    guard id >= 0, id < count else { return nil }
    let x = id % columns * Self.chunk
    let y = id / columns * Self.chunk
    return CGRect(
      x: x, y: y, width: min(Self.chunk, width - x), height: min(Self.chunk, height - y))
  }
  func frame(_ id: Int) -> CGRect? {
    rect(id).map {
      CGRect(x: $0.minX, y: CGFloat(height) - $0.maxY, width: $0.width, height: $0.height)
    }
  }
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

enum LensInput {
  static let line: CGFloat = 16
  static let escape: UInt16 = 53
  static let capsLock: UInt16 = 57

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

  static func scroll(dx: CGFloat, dy: CGFloat, precise: Bool) -> (dx: Double, dy: Double) {
    let unit = precise ? 1 : line
    return (-dx * unit, -dy * unit)
  }

  enum KeyDown: Equatable { case close, copy, send }
  static func keyDown(_ keyCode: UInt16, characters: String?, flags: NSEvent.ModifierFlags)
    -> KeyDown
  {
    if keyCode == escape { return .close }
    let held = flags.intersection([.command, .control, .option, .shift])
    if held == .command, characters?.lowercased() == "c" { return .copy }
    return .send
  }

  static func modifier(_ keyCode: UInt16, flags: UInt) -> Bool? {
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
    if flags & sides == 0 { return flags & family.rawValue != 0 }
    return flags & side != 0
  }
}
