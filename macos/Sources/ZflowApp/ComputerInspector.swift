import AppKit
import SwiftUI

struct ComputerInspector: View {
  var model: AppModel
  var snapshot: Snapshot
  var layout: Layout
  @State private var forgetting: String?

  var body: some View {
    let tiles = LayoutTile.tiles(layout, peers: snapshot.peers, ownMark: snapshot.ownMark)
    let islands = Island.islands(tiles)
    Group {
      if let island = islands.first(where: { $0.owner == model.selection }) {
        selected(island, islands: islands)
      } else {
        overview(islands)
      }
    }
    .confirmationDialog(
      "Forget \(forgetting ?? "")?",
      isPresented: Binding(get: { forgetting != nil }, set: { if !$0 { forgetting = nil } }),
      presenting: forgetting
    ) { name in
      Button("Forget", role: .destructive) {
        model.send(CoreRequest(command: "forget", name: name))
      }
    } message: { _ in
      Text(
        "This stops input between the two computers and removes its trusted identity. It goes back to Found on your network."
      )
    }
  }

  private func overview(_ islands: [Island]) -> some View {
    InspectorSection(
      title: "Your Computers",
      footer:
        "Select a computer on the arrangement or here to see its crossings, screens and settings."
    ) {
      ForEach(Array(islands.enumerated()), id: \.element.id) { index, island in
        if index > 0 { Divider().padding(.leading, 44) }
        Button {
          model.selection = island.owner
        } label: {
          HStack(spacing: 12) {
            Image(systemName: island.symbol).font(.title3).foregroundStyle(.secondary)
              .frame(width: 22)
            VStack(alignment: .leading, spacing: 1) {
              Text(island.name).fontWeight(.semibold)
              Text(summary(island)).font(.caption).foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            if let state = island.state { StateDot(state: state) }
            if let mark = island.mark { KeyMark(mark: mark) }
            Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
          }
          .padding(.horizontal, 12).padding(.vertical, 8)
          .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityValue(Text(summary(island)))
      }
    }
  }

  private func summary(_ island: Island) -> String {
    let count = island.tiles.count
    let screens = "\(count) \(count == 1 ? "screen" : "screens")"
    return [screens, island.state?.label].compactMap { $0 }.joined(separator: " · ")
  }

  @ViewBuilder
  private func selected(_ island: Island, islands: [Island]) -> some View {
    let peer = island.owner.peer.flatMap { name in snapshot.peers.first { $0.name == name } }
    VStack(alignment: .leading, spacing: 18) {
      header(island, peer: peer)
      actions(island, peer: peer)
      crossings(island, islands: islands)
      screens(island)
      if let peer { settings(peer) }
    }
  }

  private func header(_ island: Island, peer: Peer?) -> some View {
    HStack(spacing: 12) {
      Image(systemName: island.symbol).font(.system(size: 30)).foregroundStyle(.secondary)
      VStack(alignment: .leading, spacing: 2) {
        Text(island.name).font(.title2.weight(.semibold))
        HStack(spacing: 6) {
          if let peer {
            StateDot(state: peer.state)
            Text(peer.detail).textSelection(.enabled)
          } else {
            Text("This Mac")
          }
        }
        .font(.callout).foregroundStyle(.secondary)
      }
      Spacer(minLength: 8)
      if let mark = island.mark { KeyMark(mark: mark) }
      Button("Done", systemImage: "xmark") { model.selection = nil }
        .labelStyle(.iconOnly).buttonStyle(.borderless)
        .help("Show all computers")
    }
    .accessibilityElement(children: .contain)
  }

  private func actions(_ island: Island, peer: Peer?) -> some View {
    let adjusting = model.adjusting == island.owner
    return HStack(spacing: 8) {
      if island.tiles.count > 1 {
        Button(
          adjusting ? "Done Adjusting" : "Adjust Screens",
          systemImage: adjusting ? "lock.open" : "lock"
        ) {
          model.selection = island.owner
          model.adjusting = adjusting ? nil : island.owner
        }
      }
      if peer == nil {
        Button("Displays…") {
          NSWorkspace.shared.open(
            URL(string: "x-apple.systempreferences:com.apple.Displays-Settings.extension")!)
        }
      }
      Spacer()
      if let peer {
        Button("Forget…", role: .destructive) { forgetting = peer.name }
      }
    }
  }

  private func crossings(_ island: Island, islands: [Island]) -> some View {
    let mine = Set(island.tiles.map(\.id))
    let screens = Dictionary(
      islands.flatMap(\.tiles).map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
    let crossings = snapshot.seams.filter {
      !$0.sameComputer && mine.contains($0.a) != mine.contains($0.b)
    }
    return InspectorSection(
      title: "Crossings",
      footer: island.owner == .thisMac
        ? "Between this Mac’s own screens, the pointer follows System Settings › Displays."
        : nil
    ) {
      if crossings.isEmpty {
        Text("None yet. Move it until an edge touches another computer’s screen.")
          .foregroundStyle(.secondary)
          .padding(.horizontal, 12).padding(.vertical, 10)
      }
      ForEach(Array(crossings.enumerated()), id: \.offset) { index, seam in
        let here = mine.contains(seam.a) ? seam.a : seam.b
        let there = here == seam.a ? seam.b : seam.a
        let side =
          seam.vertical ? (here == seam.a ? "Right" : "Left") : (here == seam.a ? "Bottom" : "Top")
        let other = screens[there]
        if index > 0 { Divider().padding(.leading, 44) }
        HStack(spacing: 12) {
          Image(systemName: seam.vertical ? "arrow.left.arrow.right" : "arrow.up.arrow.down")
            .foregroundStyle(.tint).frame(width: 22)
          VStack(alignment: .leading, spacing: 1) {
            Text(other.map { Owner($0.computer.peer).peer ?? "This Mac" } ?? there)
              .fontWeight(.semibold)
            Text("\(side) edge of \(name(screens[here])) → \(name(other))")
              .font(.caption).foregroundStyle(.secondary)
          }
          Spacer(minLength: 0)
        }
        .padding(.horizontal, 12).padding(.vertical, 8)
        .accessibilityElement(children: .combine)
      }
    }
  }

  private func name(_ tile: LayoutTile?) -> String {
    tile.map { $0.computer.display?.name ?? $0.computer.label } ?? "a screen"
  }

  private func screens(_ island: Island) -> some View {
    InspectorSection(title: "Screens") {
      ForEach(Array(island.tiles.enumerated()), id: \.element.id) { index, tile in
        if index > 0 { Divider().padding(.leading, 44) }
        HStack(spacing: 12) {
          Image(systemName: "display").foregroundStyle(.secondary).frame(width: 22)
          VStack(alignment: .leading, spacing: 1) {
            Text(tile.computer.display?.name ?? island.name).fontWeight(.semibold)
            if let detail = Self.detail(tile.computer.display) {
              Text(detail).font(.caption).foregroundStyle(.secondary)
            }
          }
          Spacer(minLength: 0)
        }
        .padding(.horizontal, 12).padding(.vertical, 8)
        .accessibilityElement(children: .combine)
      }
    }
  }

  nonisolated static func detail(_ display: DisplayInfo?) -> String? {
    guard let display else { return nil }
    var parts: [String] = []
    if let width = display.widthMm, let height = display.heightMm, width > 0, height > 0 {
      let inches = Double(width * width + height * height).squareRoot() / 25.4
      parts.append("\(inches.formatted(.number.precision(.fractionLength(0...1))))″")
    }
    if let bounds = display.bounds {
      parts.append("\(bounds.width) × \(bounds.height)")
    }
    return parts.isEmpty ? nil : parts.joined(separator: " · ")
  }

  private func settings(_ peer: Peer) -> some View {
    let name = peer.name
    return InspectorSection(
      title: "Settings",
      footer: "\(peer.keyboard.detail) Changes apply the next time \(name) takes control."
    ) {
      row("\(name) can control this Mac") {
        Toggle(
          "\(name) can control this Mac",
          isOn: binding(peer.allowControl) {
            CoreRequest(command: "set_peer", name: name, allowControl: $0)
          }
        )
        .toggleStyle(.switch).controlSize(.small)
      }
      Divider().padding(.leading, 12)
      row("Keys from \(name)") {
        Picker(
          "Keys from \(name)",
          selection: binding(peer.keyboard) {
            CoreRequest(command: "set_peer", name: name, keyboard: $0)
          }
        ) {
          ForEach(KeyboardMode.allCases, id: \.self) { Text($0.title).tag($0) }
        }
        .fixedSize()
      }
      Divider().padding(.leading, 12)
      row("Reverse scrolling") {
        Toggle(
          "Reverse scrolling",
          isOn: binding(peer.reverseScroll) {
            CoreRequest(command: "set_peer", name: name, reverseScroll: $0)
          }
        )
        .toggleStyle(.switch).controlSize(.small)
      }
    }
  }

  private func row<Control: View>(_ title: String, @ViewBuilder control: () -> Control)
    -> some View
  {
    HStack {
      Text(title)
      Spacer(minLength: 12)
      control().labelsHidden()
    }
    .padding(.horizontal, 12).padding(.vertical, 7)
  }

  private func binding<Value>(
    _ value: Value, _ request: @escaping (Value) -> CoreRequest
  ) -> Binding<Value> {
    Binding(get: { value }, set: { model.send(request($0)) })
  }
}

private struct InspectorSection<Content: View>: View {
  var title: String
  var footer: String? = nil
  @ViewBuilder var content: () -> Content

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      Text(title).font(.headline)
      VStack(alignment: .leading, spacing: 0) { content() }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.quinary, in: .rect(cornerRadius: 12))
      if let footer {
        Text(footer).font(.callout).foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
    }
  }
}

extension KeyboardMode {
  var title: String {
    switch self {
    case .standard: "Standard keys"
    case .pcPositions: "PC key positions"
    case .mac: "Mac shortcuts"
    }
  }
  var detail: String {
    switch self {
    case .standard: "Keys arrive as sent: Super is Command and Alt is Option."
    case .pcPositions: "Alt and Super trade places, so each key acts like the Mac key in its spot."
    case .mac:
      "Picked for a PC keyboard: Ctrl and Command trade places, so Ctrl+C copies, except in terminals."
    }
  }
}
