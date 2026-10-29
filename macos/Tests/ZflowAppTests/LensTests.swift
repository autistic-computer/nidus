import AppKit
import Foundation
import Testing

@testable import ZflowApp

private func near(_ a: CGPoint, _ b: CGPoint) -> Bool {
  abs(a.x - b.x) < 1e-6 && abs(a.y - b.y) < 1e-6
}

@Test func aWideDesktopFitsWithBarsAboveAndBelow() throws {
  let fit = try #require(
    LensFit(
      desktop: CGSize(width: 3840, height: 2160), in: CGRect(x: 0, y: 0, width: 1512, height: 982)))
  #expect(fit.scale == 1512.0 / 3840)
  #expect(fit.frame.minX == 0 && fit.frame.width == 1512)
  #expect(abs(fit.frame.minY - (982 - 850.5) / 2) < 1e-9)
  #expect(near(fit.point(.zero), CGPoint(x: 0, y: fit.frame.maxY)))
  #expect(near(fit.point(CGPoint(x: 3840, y: 2160)), CGPoint(x: 1512, y: fit.frame.minY)))
  #expect(abs(fit.pixels(200) - 200 / fit.scale) < 1e-9)
}

@Test func aTallerDesktopFitsWithBarsAtTheSides() throws {
  let fit = try #require(
    LensFit(
      desktop: CGSize(width: 1920, height: 1200), in: CGRect(x: 0, y: 0, width: 1920, height: 1080))
  )
  #expect(fit.scale == 0.9)
  #expect(fit.frame == CGRect(x: 96, y: 0, width: 1728, height: 1080))
  #expect(LensFit(desktop: .zero, in: CGRect(x: 0, y: 0, width: 10, height: 10)) == nil)
}

@Test func pointsAndPixelsRoundTrip() throws {
  let fit = try #require(
    LensFit(
      desktop: CGSize(width: 3840, height: 2160), in: CGRect(x: 0, y: 0, width: 1512, height: 982)))
  for pixel in [
    CGPoint(x: 0, y: 0), CGPoint(x: 1919.5, y: 1080), CGPoint(x: 3839, y: 2159),
    CGPoint(x: 17, y: 2000),
  ] {
    #expect(near(fit.pixel(fit.point(pixel)), pixel), "\(pixel)")
  }
  for point in [CGPoint(x: 0, y: 65.75), CGPoint(x: 756, y: 491), CGPoint(x: 1511, y: 916)] {
    #expect(near(fit.point(fit.pixel(point)), point), "\(point)")
  }
}

@Test func thePointerInTheBarsHoldsToTheDesktopEdge() throws {
  let fit = try #require(
    LensFit(
      desktop: CGSize(width: 3840, height: 2160), in: CGRect(x: 0, y: 0, width: 1512, height: 982)))
  let above = fit.pixel(CGPoint(x: 756, y: 975))
  #expect(above.y < 0)
  #expect(fit.clamped(above) == CGPoint(x: above.x, y: 0))
  #expect(fit.clamped(fit.pixel(CGPoint(x: 1600, y: 10))) == CGPoint(x: 3839, y: 2159))
  let inside = CGPoint(x: 100.25, y: 200.5)
  #expect(fit.clamped(inside) == inside)
}

@Test func theGridMatchesTheEnginesChunks() throws {
  let grid = LensGrid(width: 3840, height: 2160)
  #expect(LensGrid.chunk == 256)
  #expect((grid.columns, grid.rows, grid.count) == (15, 9, 135))
  #expect(grid.rect(0) == CGRect(x: 0, y: 0, width: 256, height: 256))
  #expect(grid.rect(134) == CGRect(x: 3584, y: 2048, width: 256, height: 112))
  #expect(grid.rect(135) == nil && grid.rect(-1) == nil)
  #expect(grid.frame(0) == CGRect(x: 0, y: 1904, width: 256, height: 256))
  #expect(grid.frame(134) == CGRect(x: 3584, y: 0, width: 256, height: 112))
  let frames = (0..<grid.count).compactMap(grid.frame)
  let area = frames.reduce(CGFloat(0)) { $0 + $1.width * $1.height }
  #expect(area == CGFloat(3840 * 2160))
  #expect(frames.allSatisfy { CGRect(x: 0, y: 0, width: 3840, height: 2160).contains($0) })
}

@Test func aCircleCoversTheChunksUnderItsSquare() throws {
  let small = LensGrid(width: 600, height: 300)
  #expect(small.chunks(around: CGPoint(x: 100, y: 100), radius: 50) == [0])
  #expect(small.chunks(around: CGPoint(x: 590, y: 290), radius: 20) == [5])
  #expect(small.rect(5) == CGRect(x: 512, y: 256, width: 88, height: 44))

  let grid = LensGrid(width: 3840, height: 2160)
  #expect(grid.chunks(around: CGPoint(x: 256, y: 256), radius: 10) == [0, 1, 15, 16])
  #expect(grid.chunks(around: CGPoint(x: 5000, y: 100), radius: 100) == [])
  #expect(grid.chunks(around: CGPoint(x: CGFloat.nan, y: 0), radius: 10) == [])
  let fit = try #require(
    LensFit(
      desktop: CGSize(width: 3840, height: 2160), in: CGRect(x: 0, y: 0, width: 1512, height: 982)))
  let radius = fit.pixels(200)
  for center in [CGPoint(x: 3839, y: 2159), CGPoint(x: 1920, y: 1080), CGPoint(x: -300, y: 40)] {
    let square = CGRect(
      x: center.x - radius, y: center.y - radius, width: radius * 2, height: radius * 2)
    let ids = grid.chunks(around: center, radius: radius)
    #expect(!ids.isEmpty)
    #expect(ids.count <= 25)
    for id in ids {
      let rect = try #require(grid.rect(id))
      #expect(rect.intersects(square), "\(id) at \(center)")
    }
  }
}

@Test func buttonsMapToTheEnginesNumbers() {
  #expect((0...4).map(LensInput.button) == [1, 3, 2, 4, 5])
  #expect(LensInput.button(5) == nil)
}

@Test func scrollingMovesContentTheWayItMovesHere() {
  let swipe = LensInput.scroll(dx: 3, dy: -10, precise: true)
  #expect(swipe.dx == -3 && swipe.dy == 10)
  let wheel = LensInput.scroll(dx: 0, dy: 1, precise: false)
  #expect(wheel.dx == 0 && wheel.dy == -16)
}

@Test func escapeClosesAndCommandCCopies() {
  #expect(LensInput.keyDown(53, characters: "\u{1b}", flags: []) == .close)
  #expect(LensInput.keyDown(8, characters: "c", flags: .command) == .copy)
  #expect(LensInput.keyDown(8, characters: "C", flags: [.command, .capsLock]) == .copy)
  #expect(LensInput.keyDown(34, characters: "c", flags: .command) == .copy)
  #expect(LensInput.keyDown(8, characters: "c", flags: [.command, .shift]) == .send)
  #expect(LensInput.keyDown(8, characters: "c", flags: .control) == .send)
  #expect(LensInput.keyDown(8, characters: "c", flags: []) == .send)
  #expect(LensInput.keyDown(9, characters: "v", flags: .command) == .send)
}

@Test func modifiersReadEachSide() {
  let shift = NSEvent.ModifierFlags.shift.rawValue
  #expect(LensInput.modifier(56, flags: shift | 0x02) == true)
  #expect(LensInput.modifier(60, flags: shift | 0x06) == true)
  #expect(LensInput.modifier(56, flags: shift | 0x04) == false)
  #expect(LensInput.modifier(60, flags: 0) == false)
  #expect(LensInput.modifier(55, flags: NSEvent.ModifierFlags.command.rawValue) == true)
  #expect(LensInput.modifier(62, flags: NSEvent.ModifierFlags.control.rawValue | 0x2000) == true)
  #expect(LensInput.modifier(59, flags: NSEvent.ModifierFlags.control.rawValue | 0x2000) == false)
  #expect(LensInput.modifier(63, flags: NSEvent.ModifierFlags.function.rawValue) == true)
  #expect(LensInput.modifier(57, flags: NSEvent.ModifierFlags.capsLock.rawValue) == nil)
  #expect(LensInput.modifier(0, flags: shift) == nil)
}

@Test func theLensRequestNamesTheComputer() throws {
  let encoder = JSONEncoder()
  encoder.keyEncodingStrategy = .convertToSnakeCase
  encoder.outputFormatting = .sortedKeys
  let json = try encoder.encode(CoreRequest(command: "lens", name: "desk"))
  #expect(String(decoding: json, as: UTF8.self) == #"{"command":"lens","name":"desk"}"#)
}

private func peer(_ name: String, _ state: Peer.State) -> Peer {
  Peer(
    name: name, state: state, detail: "", allowControl: true, keyboard: .standard,
    reverseScroll: false, mark: "0123ab")
}

@Test func theHotKeyLooksAgainAtTheLastComputerWhileItIsConnected() {
  let peers = [
    peer("desk", .paired), peer("laptop", .connected), peer("tower", .controlledFromHere),
    peer("studio", .controllingThis),
  ]
  #expect(AppModel.lensTarget(peers: peers, last: "tower") == "tower")
  #expect(AppModel.lensTarget(peers: peers, last: "studio") == "studio")
  #expect(AppModel.lensTarget(peers: peers, last: nil) == "laptop")
  #expect(AppModel.lensTarget(peers: peers, last: "desk") == "laptop")
  #expect(AppModel.lensTarget(peers: peers, last: "gone") == "laptop")
}

@Test func theHotKeyHasNothingToLookAtWithoutAConnectedComputer() {
  let peers = [peer("desk", .unreachable), peer("laptop", .connecting), peer("tower", .paired)]
  #expect(AppModel.lensTarget(peers: peers, last: "desk") == nil)
  #expect(AppModel.lensTarget(peers: [], last: nil) == nil)
}
