import SwiftUI

struct LayoutTile: Identifiable, Equatable {
  var computer: Computer
  var state: Peer.State?
  var mark: String?
  var placed = true
  var id: String { computer.id }
  var owner: Owner { Owner(computer.peer) }

  static func tiles(_ layout: Layout, peers: [Peer], ownMark: String? = nil) -> [LayoutTile] {
    layout.monitors.filter { $0.display?.active != false }.map { computer in
      guard let name = computer.peer else {
        return LayoutTile(computer: computer, state: nil, mark: ownMark)
      }
      let peer = peers.first { $0.name == name }
      return LayoutTile(computer: computer, state: peer?.state, mark: peer?.mark)
    }
  }
}

struct Island: Identifiable, Equatable {
  var owner: Owner
  var tiles: [LayoutTile]
  var id: Owner { owner }
  var name: String { owner.peer ?? "This Mac" }
  var state: Peer.State? { tiles.first?.state }
  var mark: String? { tiles.first?.mark }
  var symbol: String { owner == .thisMac ? "laptopcomputer" : "desktopcomputer" }

  static func islands(_ tiles: [LayoutTile]) -> [Island] {
    var islands: [Island] = []
    for tile in tiles {
      if let index = islands.firstIndex(where: { $0.owner == tile.owner }) {
        islands[index].tiles.append(tile)
      } else {
        islands.append(Island(owner: tile.owner, tiles: [tile]))
      }
    }
    return islands
  }
}

struct Move: Equatable, Sendable {
  var id: String
  var x: Int
  var y: Int
  var tolerance: Int
  var computer: Bool

  var request: CoreRequest { coreRequest("move_tile") }
  var preview: CoreRequest { coreRequest("preview") }
  private func coreRequest(_ command: String) -> CoreRequest {
    CoreRequest(command: command, id: id, x: x, y: y, tolerance: tolerance, computer: computer)
  }
}

struct Drag: Equatable {
  var anchor: Computer
  var owner: Owner
  var computer: Bool
  var scale: CGFloat
  var origin: CGPoint
  var travel = CGSize.zero
  var asked: CGSize?
  var landing: Preview?

  func moves(_ tile: LayoutTile) -> Bool {
    computer ? tile.owner == owner : tile.id == anchor.id
  }
  var refused: Bool { asked != nil && landing == nil }
  var shown: CGSize {
    guard let landing else { return travel }
    return CGSize(width: landing.x - anchor.x, height: landing.y - anchor.y)
  }
  func move(tolerance: Int) -> Move {
    Move(
      id: anchor.id, x: anchor.x + Int(travel.width.rounded()),
      y: anchor.y + Int(travel.height.rounded()), tolerance: tolerance, computer: computer)
  }
}

extension Computer {
  var rect: CGRect { CGRect(x: x, y: y, width: width, height: height) }
}

enum ShelfEdge {
  case bottom
  case trailing
}

struct ComputerLayout: View {
  var tiles: [LayoutTile]
  var seams: [Seam] = []
  var sharing = true
  var unplaced: [Unplaced] = []
  var shelf = ShelfEdge.bottom
  var searching = false
  @Binding var selection: Owner?
  @Binding var adjusting: Owner?
  var move: @MainActor (Move) async -> Void
  var preview: @MainActor (Move) async -> Preview? = { _ in nil }
  var place: (Unplaced, Int, Int, Int) -> Void = { _, _, _, _ in }
  @State private var drag: Drag?
  @State private var asking = false
  @State private var carried: String?
  @State private var carriedAt = CGPoint.zero
  @State private var hoveredScreen: String?
  @State private var hoveredChip: Owner?
  @FocusState private var focused: Bool

  nonisolated static let newTile = CGSize(width: 1920, height: 1080)
  nonisolated static let step = 100
  private static let space = "canvas"

  private var placed: [LayoutTile] { tiles.filter(\.placed) }
  private var islands: [Island] { Island.islands(placed) }
  private var showsShelf: Bool { !unplaced.isEmpty || searching }
  private var hovered: Owner? {
    hoveredChip ?? placed.first { $0.id == hoveredScreen }?.owner
  }

  var body: some View {
    GeometryReader { area in
      let shelfFrame = shelfRect(area.size)
      let board = boardRect(area.size, shelf: shelfFrame)
      let fit = drag.map { ($0.scale, $0.origin) } ?? Self.fit(layoutBounds, in: board)
      let scale = fit.0
      let origin = fit.1
      ZStack {
        RoundedRectangle(cornerRadius: 16).fill(.quinary)
          .onTapGesture {
            selection = nil
            adjusting = nil
          }
        dots
        if placed.isEmpty {
          Text("Detecting this Mac’s displays…").foregroundStyle(.secondary)
            .position(x: board.midX, y: board.midY)
        }
        ForEach(islands) { island in
          hull(island, scale: scale, origin: origin)
        }
        ForEach(placed) { tile in
          screen(tile, scale: scale, origin: origin)
        }
        seamLines(scale: scale, origin: origin)
        chips(scale: scale, origin: origin)
        if let shelfFrame {
          shelfView(scale: scale, origin: origin, board: board, frame: shelfFrame)
            .frame(width: shelfFrame.width, height: shelfFrame.height)
            .position(x: shelfFrame.midX, y: shelfFrame.midY)
        }
        if let computer = unplaced.first(where: { $0.id == carried }) {
          if board.contains(carriedAt) && !(shelfFrame?.contains(carriedAt) ?? false) {
            RoundedRectangle(cornerRadius: 10)
              .strokeBorder(Color.accentColor, style: StrokeStyle(lineWidth: 1.5, dash: [5, 4]))
              .frame(width: Self.newTile.width * scale, height: Self.newTile.height * scale)
              .position(carriedAt)
          }
          ShelfTile(computer: computer, carried: true)
            .position(carriedAt)
            .allowsHitTesting(false)
        }
      }
      .coordinateSpace(.named(Self.space))
      .clipShape(.rect(cornerRadius: 16))
      .animation(.smooth(duration: 0.3), value: tiles)
    }
    .focusable()
    .focused($focused)
    .focusEffectDisabled()
    .onKeyPress(keys: [.leftArrow, .rightArrow, .upArrow, .downArrow]) { press in
      guard let island = islands.first(where: { $0.owner == selection }) else { return .ignored }
      let (dx, dy) = Self.direction(press.key)
      nudge(island.tiles[0].computer, dx, dy, computer: true)
      return .handled
    }
    .onKeyPress(.escape) {
      guard selection != nil || adjusting != nil else { return .ignored }
      selection = nil
      adjusting = nil
      return .handled
    }
  }

  nonisolated static func fit(_ bounds: CGRect, in board: CGRect) -> (CGFloat, CGPoint) {
    let (sides, above, below): (CGFloat, CGFloat, CGFloat) = (36, 52, 30)
    let scale = max(
      0.001,
      min(
        (board.width - 2 * sides) / max(bounds.width, 1),
        (board.height - above - below) / max(bounds.height, 1), 0.12))
    let room = board.height - above - below - bounds.height * scale
    let origin = CGPoint(
      x: board.midX - bounds.width * scale / 2 - bounds.minX * scale,
      y: board.minY + above + room / 2 - bounds.minY * scale)
    return (scale, origin)
  }

  nonisolated static func tolerance(_ scale: CGFloat) -> Int {
    Int((CGFloat(14) / scale).rounded())
  }

  static func direction(_ key: KeyEquivalent) -> (Int, Int) {
    switch key {
    case .leftArrow: (-1, 0)
    case .rightArrow: (1, 0)
    case .upArrow: (0, -1)
    default: (0, 1)
    }
  }

  private var dots: some View {
    Canvas { context, size in
      let spacing: CGFloat = 18
      var path = Path()
      for x in stride(from: spacing / 2, to: size.width, by: spacing) {
        for y in stride(from: spacing / 2, to: size.height, by: spacing) {
          path.addEllipse(in: CGRect(x: x - 0.75, y: y - 0.75, width: 1.5, height: 1.5))
        }
      }
      context.fill(path, with: .color(.secondary.opacity(0.15)))
    }
    .accessibilityHidden(true)
  }

  private func shelfRect(_ size: CGSize) -> CGRect? {
    guard showsShelf else { return nil }
    switch shelf {
    case .bottom: return CGRect(x: 14, y: size.height - 122, width: size.width - 28, height: 108)
    case .trailing: return CGRect(x: size.width - 272, y: 22, width: 250, height: size.height - 44)
    }
  }

  private func boardRect(_ size: CGSize, shelf frame: CGRect?) -> CGRect {
    let all = CGRect(origin: .zero, size: size)
    guard let frame else { return all }
    switch shelf {
    case .bottom: return CGRect(x: 0, y: 0, width: size.width, height: frame.minY)
    case .trailing: return CGRect(x: 0, y: 0, width: frame.minX, height: size.height)
    }
  }

  private func shelfView(scale: CGFloat, origin: CGPoint, board: CGRect, frame: CGRect)
    -> some View
  {
    let found = ForEach(unplaced) { computer in
      shelfTile(computer, scale: scale, origin: origin, board: board, shelf: frame)
    }
    let title = Text("Found on your network").font(.callout.weight(.semibold))
    let looking = HStack(spacing: 8) {
      ProgressView().controlSize(.small)
      Text("Looking for computers…").foregroundStyle(.secondary)
    }
    .font(.callout)
    return Group {
      switch shelf {
      case .bottom:
        HStack(spacing: 18) {
          VStack(alignment: .leading, spacing: 3) {
            title
            Text("Drag one next to a screen to add it. Nothing else to do.")
              .font(.callout).foregroundStyle(.secondary)
          }
          .frame(maxWidth: .infinity, alignment: .leading)
          if unplaced.isEmpty { looking }
          found
        }
        .padding(.horizontal, 14)
      case .trailing:
        VStack(alignment: .leading, spacing: 10) {
          title.foregroundStyle(.secondary)
          if unplaced.isEmpty { looking }
          found
          Spacer(minLength: 0)
          if unplaced.contains(where: \.placeable) {
            Text("Drag it next to This Mac.").font(.caption).foregroundStyle(.secondary)
          }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background(.background.opacity(0.4), in: .rect(cornerRadius: 12))
    .overlay {
      RoundedRectangle(cornerRadius: 12)
        .strokeBorder(
          Color.secondary.opacity(0.4), style: StrokeStyle(lineWidth: 1.5, dash: [6, 4]))
    }
  }

  private func shelfTile(
    _ computer: Unplaced, scale: CGFloat, origin: CGPoint, board: CGRect, shelf: CGRect
  ) -> some View {
    let local = placed.first { $0.computer.peer == nil }?.computer
    return ShelfTile(computer: computer, carried: false)
      .frame(maxWidth: self.shelf == .trailing ? .infinity : 150)
      .opacity(carried == computer.id ? 0.35 : 1)
      .gesture(
        DragGesture(minimumDistance: 2, coordinateSpace: .named(Self.space))
          .onChanged { value in
            carried = computer.id
            carriedAt = value.location
          }
          .onEnded { value in
            carried = nil
            let at = value.location
            guard board.contains(at), !shelf.contains(at) else { return }
            let (x, y) = Self.corner(at: at, scale: scale, origin: origin)
            place(computer, x, y, Int((CGFloat(14) / scale).rounded()))
          },
        isEnabled: computer.placeable
      )
      .accessibilityElement(children: .ignore)
      .accessibilityLabel(Text(computer.name))
      .accessibilityValue(Text(computer.detail))
      .accessibilityAction(named: Text("Place beside This Mac")) {
        guard computer.placeable else { return }
        let x = local.map { $0.x + $0.width } ?? 0
        place(computer, x, local?.y ?? 0, 150)
      }
  }

  nonisolated static func corner(at point: CGPoint, scale: CGFloat, origin: CGPoint) -> (Int, Int) {
    let x = (point.x - origin.x) / scale - newTile.width / 2
    let y = (point.y - origin.y) / scale - newTile.height / 2
    return (Int(x.rounded()), Int(y.rounded()))
  }

  nonisolated static func frame(for computer: Computer, scale: CGFloat, origin: CGPoint) -> CGRect {
    CGRect(
      x: origin.x + CGFloat(computer.x) * scale,
      y: origin.y + CGFloat(computer.y) * scale,
      width: CGFloat(computer.width) * scale,
      height: CGFloat(computer.height) * scale)
  }

  private func screenFrame(_ tile: LayoutTile, scale: CGFloat, origin: CGPoint) -> CGRect {
    let frame = Self.frame(for: tile.computer, scale: scale, origin: origin)
    guard let drag, drag.moves(tile) else { return frame }
    return frame.offsetBy(dx: drag.shown.width * scale, dy: drag.shown.height * scale)
  }

  private func hullFrame(_ island: Island, scale: CGFloat, origin: CGPoint) -> CGRect {
    island.tiles.reduce(CGRect.null) { $0.union(screenFrame($1, scale: scale, origin: origin)) }
      .insetBy(dx: -10, dy: -10)
  }

  private func hull(_ island: Island, scale: CGFloat, origin: CGPoint) -> some View {
    let frame = hullFrame(island, scale: scale, origin: origin)
    let selected = selection == island.owner
    return RoundedRectangle(cornerRadius: 18)
      .strokeBorder(
        selected ? Color.accentColor : Color.secondary.opacity(0.35),
        style: StrokeStyle(
          lineWidth: selected ? 2.5 : 1.5, dash: adjusting == island.owner ? [6, 5] : []))
      .opacity(selected || hovered == island.owner ? 1 : 0)
      .frame(width: frame.width, height: frame.height)
      .position(x: frame.midX, y: frame.midY)
      .allowsHitTesting(false)
  }

  private func screen(_ tile: LayoutTile, scale: CGFloat, origin: CGPoint) -> some View {
    let frame = screenFrame(tile, scale: scale, origin: origin)
    let moving = drag?.moves(tile) ?? false
    let alone = adjusting == tile.owner
    let island = islands.first { $0.owner == tile.owner }
    let computer = tile.computer
    return ScreenView(
      tile: tile, size: frame.size,
      showsName: selection == tile.owner || hovered == tile.owner, moving: moving,
      refused: moving && drag?.refused == true
    )
    .position(x: frame.midX, y: frame.midY)
    .zIndex(moving ? 1 : 0)
    .onHover { inside in
      if inside {
        hoveredScreen = tile.id
      } else if hoveredScreen == tile.id {
        hoveredScreen = nil
      }
    }
    .gesture(dragGesture(computer, owner: tile.owner, computer: !alone, scale: scale, origin: origin))
    .onTapGesture(count: 2) { toggleAdjusting(tile.owner) }
    .onTapGesture { select(tile.owner) }
    .accessibilityElement(children: .ignore)
    .accessibilityLabel(Text("\(computer.display?.name ?? computer.label), \(island?.name ?? "")"))
    .accessibilityValue(Text("Position \(computer.x), \(computer.y)"))
    .accessibilityAction(named: Text("Move left")) { nudge(computer, -1, 0, computer: !alone) }
    .accessibilityAction(named: Text("Move right")) { nudge(computer, 1, 0, computer: !alone) }
    .accessibilityAction(named: Text("Move up")) { nudge(computer, 0, -1, computer: !alone) }
    .accessibilityAction(named: Text("Move down")) { nudge(computer, 0, 1, computer: !alone) }
  }

  private func seamLines(scale: CGFloat, origin: CGPoint) -> some View {
    let moving = Set(placed.filter { drag?.moves($0) ?? false }.map(\.id))
    let still = seams.filter { !moving.contains($0.a) && !moving.contains($0.b) }
    let shown = drag?.landing?.seams ?? (drag == nil ? seams : still)
    let strength = sharing ? 1.0 : 0.3
    return Canvas { context, _ in
      for seam in shown where seam.sameComputer {
        context.stroke(
          Self.line(seam, inset: 4, scale: scale, origin: origin), with: .color(.secondary),
          style: StrokeStyle(lineWidth: 2, lineCap: .round, dash: [1, 5]))
      }
      for seam in shown where !seam.sameComputer {
        let line = Self.line(seam, inset: 3, scale: scale, origin: origin)
        context.stroke(
          line, with: .color(.accentColor.opacity(0.28 * strength)),
          style: StrokeStyle(lineWidth: 11, lineCap: .round))
        context.stroke(
          line, with: .color(.accentColor.opacity(strength)),
          style: StrokeStyle(lineWidth: 4, lineCap: .round))
        let length = CGFloat(seam.end - seam.start) * scale
        guard length > 34 else { continue }
        let middle = line.boundingRect
        let center = CGPoint(x: middle.midX, y: middle.midY)
        context.fill(
          Path(ellipseIn: CGRect(x: center.x - 9, y: center.y - 9, width: 18, height: 18)),
          with: .color(.accentColor.opacity(strength)))
        var arrows = context.resolve(
          Image(systemName: seam.vertical ? "arrow.left.arrow.right" : "arrow.up.arrow.down"))
        arrows.shading = .color(.white)
        context.draw(arrows, in: CGRect(x: center.x - 5, y: center.y - 5, width: 10, height: 10))
      }
    }
    .allowsHitTesting(false)
    .accessibilityHidden(true)
  }

  nonisolated static func line(_ seam: Seam, inset: CGFloat, scale: CGFloat, origin: CGPoint)
    -> Path
  {
    let at = CGFloat(seam.at) * scale
    let start = CGFloat(seam.start) * scale + inset
    let end = max(start, CGFloat(seam.end) * scale - inset)
    var path = Path()
    if seam.vertical {
      path.move(to: CGPoint(x: origin.x + at, y: origin.y + start))
      path.addLine(to: CGPoint(x: origin.x + at, y: origin.y + end))
    } else {
      path.move(to: CGPoint(x: origin.x + start, y: origin.y + at))
      path.addLine(to: CGPoint(x: origin.x + end, y: origin.y + at))
    }
    return path
  }

  private func chips(scale: CGFloat, origin: CGPoint) -> some View {
    ZStack(alignment: .topLeading) {
      Color.clear.allowsHitTesting(false)
      ForEach(islands) { island in
        let frame = hullFrame(island, scale: scale, origin: origin)
        chip(island, scale: scale, origin: origin)
          .fixedSize()
          .offset(x: max(4, frame.minX), y: max(4, frame.minY - 32))
      }
    }
  }

  private func chip(_ island: Island, scale: CGFloat, origin: CGPoint) -> some View {
    let anchor = island.tiles[0].computer
    let open = adjusting == island.owner
    let selected = selection == island.owner
    return HStack(spacing: 7) {
      Text(island.name).font(.subheadline.weight(.semibold))
      if let state = island.state { StateDot(state: state) }
      if open {
        Button("Done") { adjusting = nil }
          .buttonStyle(.borderless).fontWeight(.semibold)
      }
    }
    .font(.callout)
    .lineLimit(1)
    .padding(.leading, 9).padding(.trailing, 11)
    .frame(height: 26)
    .background(hoveredChip == island.owner ? Color.primary.opacity(0.08) : .clear, in: .capsule)
    .glassEffect(selected ? .regular : .identity, in: .capsule)
    .overlay {
      if selected {
        Capsule().strokeBorder(Color.accentColor, lineWidth: 2)
      }
    }
    .contentShape(.capsule)
    .help(island.state.map { "\(island.name) · \($0.label)" } ?? island.name)
    .onHover { inside in
      if inside {
        hoveredChip = island.owner
      } else if hoveredChip == island.owner {
        hoveredChip = nil
      }
    }
    .gesture(dragGesture(anchor, owner: island.owner, computer: true, scale: scale, origin: origin))
    .onTapGesture(count: 2) { toggleAdjusting(island.owner) }
    .onTapGesture { select(island.owner) }
    .accessibilityElement(children: .ignore)
    .accessibilityLabel(Text(island.name))
    .accessibilityValue(Text(island.state?.label ?? "This Mac"))
    .accessibilityAddTraits(.isButton)
    .accessibilityAction { select(island.owner) }
    .accessibilityAction(named: Text("Move left")) { nudge(anchor, -1, 0, computer: true) }
    .accessibilityAction(named: Text("Move right")) { nudge(anchor, 1, 0, computer: true) }
    .accessibilityAction(named: Text("Move up")) { nudge(anchor, 0, -1, computer: true) }
    .accessibilityAction(named: Text("Move down")) { nudge(anchor, 0, 1, computer: true) }
    .accessibilityAction(named: Text(open ? "Done Adjusting Screens" : "Adjust Screens")) {
      toggleAdjusting(island.owner)
    }
  }

  private func select(_ owner: Owner) {
    selection = owner
    focused = true
  }

  private func toggleAdjusting(_ owner: Owner) {
    select(owner)
    adjusting = adjusting == owner ? nil : owner
  }

  private func nudge(_ anchor: Computer, _ dx: Int, _ dy: Int, computer: Bool) {
    let step = Self.step
    let wanted = Move(
      id: anchor.id, x: anchor.x + dx * step, y: anchor.y + dy * step, tolerance: step - 1,
      computer: computer)
    Task { await move(wanted) }
  }

  private func dragGesture(
    _ anchor: Computer, owner: Owner, computer: Bool, scale: CGFloat, origin: CGPoint
  ) -> some Gesture {
    DragGesture(minimumDistance: 2)
      .onChanged { value in
        var next =
          drag
          ?? Drag(anchor: anchor, owner: owner, computer: computer, scale: scale, origin: origin)
        if drag == nil { select(owner) }
        next.travel = CGSize(
          width: value.translation.width / next.scale,
          height: value.translation.height / next.scale)
        drag = next
        ask()
      }
      .onEnded { _ in
        guard let current = drag else { return }
        let wanted = current.move(tolerance: Self.tolerance(current.scale))
        Task {
          await move(wanted)
          drag = nil
        }
      }
  }

  private func ask() {
    guard !asking, let current = drag, current.asked != current.travel else { return }
    asking = true
    let wanted = current.move(tolerance: Self.tolerance(current.scale))
    Task {
      let landing = await preview(wanted)
      asking = false
      guard var latest = drag, latest.anchor.id == current.anchor.id else { return }
      latest.asked = current.travel
      latest.landing = landing
      drag = latest
      ask()
    }
  }

  private var layoutBounds: CGRect {
    let bounds = placed.reduce(CGRect.null) {
      $0.union(
        CGRect(
          x: $1.computer.x, y: $1.computer.y, width: $1.computer.width, height: $1.computer.height))
    }
    return bounds.isNull ? CGRect(x: 0, y: 0, width: 1600, height: 1000) : bounds
  }
}

private struct ScreenView: View {
  var tile: LayoutTile
  var size: CGSize
  var showsName: Bool
  var moving: Bool
  var refused: Bool

  var body: some View {
    let computer = tile.computer
    let away = computer.peer != nil && [nil, .paired, .unreachable].contains(tile.state)
    let shape = RoundedRectangle(cornerRadius: max(2, min(8, size.height / 8)))
    return ZStack(alignment: .bottomLeading) {
      shape.fill(Color(nsColor: .controlBackgroundColor))
      if let name = computer.display?.name {
        if showsName && size.width >= 64 && size.height >= 30 {
          Text(name).font(.caption2.weight(.semibold)).lineLimit(1)
            .padding(.horizontal, 5).padding(.vertical, 2)
            .background(.thinMaterial, in: .rect(cornerRadius: 4))
            .padding(5)
        }
      } else {
        Image(systemName: tile.owner == .thisMac ? "laptopcomputer" : "desktopcomputer")
          .font(.system(size: max(8, min(size.height * 0.35, 28))))
          .foregroundStyle(.secondary)
          .frame(maxWidth: .infinity, maxHeight: .infinity)
      }
    }
    .frame(width: size.width, height: size.height)
    .clipShape(shape)
    .overlay {
      shape.strokeBorder(
        refused ? Color.red : Color.primary.opacity(moving ? 0.35 : 0.2),
        lineWidth: refused ? 2 : 1)
    }
    .saturation(away ? 0.1 : 1)
    .opacity(away ? 0.62 : 1)
    .shadow(
      color: .black.opacity(moving ? 0.25 : 0.06), radius: moving ? 10 : 2, y: moving ? 6 : 1)
    .help(computer.label)
  }
}

private struct ShelfTile: View {
  var computer: Unplaced
  var carried: Bool

  var body: some View {
    VStack(alignment: .leading, spacing: 1) {
      HStack(alignment: .top) {
        Image(systemName: computer.symbol).font(.body).foregroundStyle(.secondary)
        Spacer(minLength: 4)
        if let mark = computer.mark {
          KeyMark(mark: mark)
        } else if computer.state == .identifying {
          ProgressView().controlSize(.mini)
        }
      }
      Spacer(minLength: 4)
      Text(computer.name).font(.callout.weight(.semibold)).lineLimit(1)
      Text(computer.detail).font(.caption).foregroundStyle(.secondary).lineLimit(1)
    }
    .padding(.vertical, 10).padding(.horizontal, 12)
    .frame(width: carried ? 150 : nil, height: 80, alignment: .topLeading)
    .frame(minWidth: 130)
    .background(Color(nsColor: .controlBackgroundColor), in: .rect(cornerRadius: 12))
    .overlay { RoundedRectangle(cornerRadius: 12).strokeBorder(Color.secondary.opacity(0.35)) }
    .opacity(computer.placeable ? 1 : 0.6)
    .shadow(color: .black.opacity(carried ? 0.18 : 0.08), radius: carried ? 10 : 6, y: 3)
    .rotationEffect(.degrees(carried ? -2 : 0))
  }
}

extension Unplaced {
  var symbol: String { os == .macos ? "laptopcomputer" : "desktopcomputer" }
  var detail: String {
    switch state {
    case .identifying: "Identifying…"
    case .differentVersion: "Different zflow version"
    case .duplicateName: "Same name as another"
    case .ready: os.map { $0 == .macos ? "macOS" : $0 == .windows ? "Windows" : "Linux" } ?? ""
    }
  }
}

struct KeyMark: View {
  var mark: String

  private nonisolated static let rgb: [UInt32] = [
    0xE5484D, 0xF76B15, 0xFFC53D, 0x30A46C, 0x12A594, 0x0090FF, 0x8E4EC6, 0xD6409F,
  ]
  nonisolated static let palette: [Color] = rgb.map(color)

  private nonisolated static func color(_ rgb: UInt32) -> Color {
    let red = Double((rgb >> 16) & 0xFF) / 255
    let green = Double((rgb >> 8) & 0xFF) / 255
    let blue = Double(rgb & 0xFF) / 255
    return Color(red: red, green: green, blue: blue)
  }
  nonisolated static func indices(_ mark: String) -> [Int] {
    mark.prefix(4).compactMap(\.hexDigitValue).map { $0 & 7 }
  }

  var body: some View {
    HStack(spacing: 2) {
      ForEach(Array(Self.indices(mark).enumerated()), id: \.offset) { _, index in
        RoundedRectangle(cornerRadius: 2).fill(Self.palette[index]).frame(width: 7, height: 7)
      }
    }
    .help("Mark \(mark)")
    .accessibilityHidden(true)
  }
}
