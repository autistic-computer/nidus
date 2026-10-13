using System.Text.Json.Nodes;
using CommunityToolkit.WinUI.Controls;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Windows.Foundation;
using Windows.System;
using Windows.UI;

namespace Zflow;

// The arrangement, as the Mac and GNOME windows show it: each computer's
// screens as an island with its name above, lit seams where the pointer
// crosses to another computer, and under it every computer, or the selected
// one's crossings, screens and settings. The engine owns the geometry: a
// drag asks it where the screens would land, and a drop asks it to move them.
public sealed partial class MainWindow
{
    // An arrow key moves a computer 100 layout units, about an inch, and snaps
    // within a little less, so a step away from a seam is not pulled back to
    // it. A drag snaps within 14 pixels.
    private const int KeyStep = 100, KeyTolerance = KeyStep - 1;
    private const double DragTolerance = 14;
    // Room around the arrangement: at the sides, above it for the computers'
    // names, and under it. An island's outline stands off its screens.
    private const double RoomSides = 28, RoomAbove = 48, RoomBelow = 24, HullGap = 8, ChipGap = 6;
    private const string Local = "local", ThisPc = "This PC";
    // Each of a key mark's first four hex digits picks one of these by its low
    // three bits, as on the Mac and GNOME, so a key looks the same everywhere.
    private static readonly Color[] MarkColors = [.. new uint[] { 0xE5484D, 0xF76B15, 0xFFC53D, 0x30A46C, 0x12A594, 0x0090FF, 0x8E4EC6, 0xD6409F }
        .Select(rgb => Color.FromArgb(255, (byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb))];

    private sealed record Fit(double Scale, int Left, int Top, double OffsetX, double OffsetY);
    // A computer, with the screens the layout gives it, in layout order.
    private sealed record Computer(string Owner, string Name, JsonObject? Peer, string? Mark, List<JsonObject> Monitors);
    private sealed class Screen(JsonObject monitor, string owner, Border tile)
    {
        public readonly JsonObject Monitor = monitor;
        public readonly string Owner = owner;
        public readonly Border Tile = tile;
        // Where it sits on the board, and where it shows while a drag carries it.
        public Rect Home, Shown;
        public string Id => Text(Monitor["id"]);
    }
    private sealed class Island(Computer computer, Rectangle hull, Border chip, Button done)
    {
        public readonly Computer Computer = computer;
        public readonly Rectangle Hull = hull;
        public readonly Border Chip = chip;
        public readonly Button Done = done;
        public readonly List<Screen> Screens = [];
        // Which corner its name took, kept through a drag so it does not jump.
        public int? Spot;
        public string Owner => Computer.Owner;
    }
    private sealed class Drag(string owner, Screen anchor, bool computer, Fit fit)
    {
        public readonly string Owner = owner, AnchorId = anchor.Id;
        public readonly int AnchorX = Number(anchor.Monitor["x"]), AnchorY = Number(anchor.Monitor["y"]);
        // Whether the whole computer moves, or only the anchor screen.
        public readonly bool Computer = computer;
        // The board keeps its scale until the drop, so nothing rescales under the pointer.
        public readonly Fit Fit = fit;
        public double Dx, Dy;
        // The move the engine last answered for, and its answer: where the
        // screens land, or null where they cannot go.
        public string? Asked;
        public JsonObject? Landing;
        public bool Dropped;
        public bool Moves(Screen screen) => Computer ? screen.Owner == Owner : screen.Id == AnchorId;
        public (int X, int Y) Target => (AnchorX + (int)Math.Round(Dx / Fit.Scale), AnchorY + (int)Math.Round(Dy / Fit.Scale));
        public int Tolerance => (int)Math.Round(DragTolerance / Fit.Scale);
    }
    private sealed record Press(Island Island, Screen Anchor, bool Computer, Point Start, uint Pointer);

    private readonly Dictionary<string, Screen> screens = [];
    private readonly List<Island> islands = [];
    private readonly List<UIElement> seamMarks = [];
    private Fit? fit;
    private Press? pressed;
    private Drag? drag;
    private bool asking;
    private string? selection, adjusting;

    private static string? Seed(JsonNode? node) => node is JsonValue value && value.TryGetValue(out string? seed) && seed.Length >= 4 && seed.All(Uri.IsHexDigit) ? seed : null;
    private static Color? Tint(string? mark) => mark is null ? null : MarkColors[Convert.ToInt32(mark[..1], 16) & 7];
    private static Color WithAlpha(Color color, double alpha) => Color.FromArgb((byte)Math.Round(255 * alpha), color.R, color.G, color.B);
    private static Color Accent => ((SolidColorBrush)Brush("AccentFillColorDefaultBrush")).Color;

    private List<JsonObject> ActiveMonitors() => (state?["layout"]?["monitors"] as JsonArray)?.OfType<JsonObject>()
        .Where(m => m["display"]?["active"] is null || Flag(m["display"]!["active"])).ToList() ?? [];

    // The computers on the arrangement, in its order, then any paired one
    // with no screen there.
    private List<Computer> Computers()
    {
        var peers = (state?["peers"] as JsonArray)?.OfType<JsonObject>().ToList() ?? [];
        JsonObject? Peer(string name) => peers.Find(p => Text(p["name"]) == name);
        var computers = new List<Computer>();
        foreach (var monitor in ActiveMonitors())
        {
            string? name = monitor["peer"] is JsonValue peer ? peer.GetValue<string>() : null;
            string owner = name is null ? Local : "peer:" + name;
            var computer = computers.Find(c => c.Owner == owner);
            if (computer is null)
            {
                computer = new Computer(owner, name ?? ThisPc, name is null ? null : Peer(name), name is null ? Seed(state?["mark"]) : Seed(Peer(name)?["mark"]), []);
                computers.Add(computer);
            }
            computer.Monitors.Add(monitor);
        }
        foreach (var peer in peers)
        {
            string name = Text(peer["name"]);
            if (!computers.Exists(c => c.Owner == "peer:" + name)) computers.Add(new Computer("peer:" + name, name, peer, Seed(peer["mark"]), []));
        }
        return computers;
    }

    // A few words for a paired computer's state, and its dot's brush; null draws a hollow dot.
    private (string Label, string? Dot) PeerState(JsonObject peer)
    {
        string name = Text(peer["name"]);
        if (Text(state?["sending"]) == name) return ("Controlled from here", "AccentFillColorDefaultBrush");
        if (Text(state?["receiving"]) == name) return ("Controlling this PC", "AccentFillColorDefaultBrush");
        if (Flag(peer["connected"])) return ("Connected", "SystemFillColorSuccessBrush");
        if (peer["status"] is JsonValue) return (Text(peer["status"]), null);
        return ("Connecting…", "SystemFillColorCautionBrush");
    }

    private FrameworkElement Dot(JsonObject peer)
    {
        string? brush = PeerState(peer).Dot;
        return new Ellipse
        {
            Width = 8, Height = 8, VerticalAlignment = VerticalAlignment.Center,
            Fill = brush is null ? null : Brush(brush),
            Stroke = brush is null ? Brush("TextFillColorSecondaryBrush") : null, StrokeThickness = 1.5,
        };
    }

    // Four small squares, so two computers with one name look different.
    private static FrameworkElement? MarkView(string? mark)
    {
        if (mark is null) return null;
        var squares = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
        foreach (char digit in mark[..4]) squares.Children.Add(new Rectangle { Width = 7, Height = 7, RadiusX = 2, RadiusY = 2, Fill = new SolidColorBrush(MarkColors[Convert.ToInt32(digit.ToString(), 16) & 7]) });
        ToolTipService.SetToolTip(squares, "Mark " + mark);
        return squares;
    }

    private static string Summary(Computer computer, string? state)
    {
        int count = computer.Monitors.Count;
        string screens = count == 0 ? "Not on the arrangement" : $"{count} {(count == 1 ? "screen" : "screens")}";
        return state is null ? screens : $"{screens} · {state}";
    }

    // "27″ · 3840 × 2160", with what the system knows of the two.
    private static string ScreenDetail(JsonNode? display)
    {
        if (display is null) return "";
        var parts = new List<string>();
        int width = Number(display["width_mm"]), height = Number(display["height_mm"]);
        if (width > 0 && height > 0) parts.Add($"{Math.Round(Math.Sqrt(width * width + height * height) / 25.4, 1)}″");
        if (display["bounds"] is JsonObject bounds) parts.Add($"{Number(bounds["width"])} × {Number(bounds["height"])}");
        return string.Join(" · ", parts);
    }

    private static string ScreenName(JsonObject? monitor) => monitor is null ? "a screen" : Text(monitor["display"]?["name"], Text(monitor["label"]));

    private void DrawArrangement()
    {
        if (dragging || Arrangement.ActualWidth <= 64) return;
        // A drop ends with the state it brought.
        if (drag is { Dropped: true }) drag = null;
        Arrangement.Children.Clear();
        screens.Clear(); islands.Clear(); seamMarks.Clear();
        var computers = Computers().Where(c => c.Monitors.Count > 0).ToList();
        var monitors = computers.SelectMany(c => c.Monitors).ToList();
        if (monitors.Count == 0) { fit = null; return; }
        fit = FitLayout(monitors, Arrangement.ActualWidth, Arrangement.Height);
        foreach (var computer in computers)
        {
            var island = NewIsland(computer);
            foreach (var monitor in computer.Monitors)
            {
                var screen = NewScreen(monitor, island);
                island.Screens.Add(screen);
                screens[screen.Id] = screen;
            }
            islands.Add(island);
        }
        PlaceArrangement();
    }

    // The scale and origin that center the layout on the board, with room
    // above it for the computers' names, as the Mac app fits it.
    private static Fit FitLayout(List<JsonObject> monitors, double width, double height)
    {
        int left = monitors.Min(m => Number(m["x"])), top = monitors.Min(m => Number(m["y"]));
        int right = monitors.Max(m => Number(m["x"]) + Number(m["width"])), bottom = monitors.Max(m => Number(m["y"]) + Number(m["height"]));
        double scale = Math.Max(0.001, Math.Min(0.12, Math.Min((width - 2 * RoomSides) / Math.Max(right - left, 1), (height - RoomAbove - RoomBelow) / Math.Max(bottom - top, 1))));
        return new Fit(scale, left, top, (width - (right - left) * scale) / 2, RoomAbove + (height - RoomAbove - RoomBelow - (bottom - top) * scale) / 2);
    }

    // Every screen rounds its edges alike, so touching edges stay touching.
    private static Rect RectOf(JsonObject monitor, Fit f, double dx = 0, double dy = 0)
    {
        double X(int v) => Math.Round(f.OffsetX + (v - f.Left) * f.Scale + dx);
        double Y(int v) => Math.Round(f.OffsetY + (v - f.Top) * f.Scale + dy);
        int x = Number(monitor["x"]), y = Number(monitor["y"]);
        double left = X(x), top = Y(y);
        return new Rect(left, top, Math.Max(1, X(x + Number(monitor["width"])) - left), Math.Max(1, Y(y + Number(monitor["height"])) - top));
    }

    private Island NewIsland(Computer computer)
    {
        var hull = new Rectangle { RadiusX = 14, RadiusY = 14, IsHitTestVisible = false };
        Canvas.SetZIndex(hull, 0);
        var content = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 7 };
        bool local = computer.Owner == Local;
        content.Children.Add(new FontIcon { Glyph = local ? "\uE7F8" : "\uE7F4", FontSize = 14, Foreground = Brush("TextFillColorSecondaryBrush") });
        content.Children.Add(new TextBlock { Text = computer.Name, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, MaxWidth = 160, TextTrimming = TextTrimming.CharacterEllipsis, VerticalAlignment = VerticalAlignment.Center, Foreground = Brush(local ? "AccentTextFillColorPrimaryBrush" : "TextFillColorPrimaryBrush") });
        // Its state is a dot here, in words in its tooltip and the inspector,
        // so names stay small on the board.
        if (computer.Peer is not null) content.Children.Add(Dot(computer.Peer));
        if (MarkView(computer.Mark) is { } mark) content.Children.Add(mark);
        var chip = new Border { Child = content, CornerRadius = new CornerRadius(13), Padding = new Thickness(9, 3, 11, 3), Background = Brush("AcrylicInAppFillColorDefaultBrush") };
        string described = computer.Peer is null ? ThisPc : $"{computer.Name} · {PeerState(computer.Peer).Label}";
        ToolTipService.SetToolTip(chip, described);
        AutomationProperties.SetName(chip, described);
        AutomationProperties.SetHelpText(chip, "Drag to move this computer. Double-click to move its screens one by one.");
        Canvas.SetZIndex(chip, 4);
        var done = new Button { Content = "Done", Style = (Style)Application.Current.Resources["AccentButtonStyle"], Padding = new Thickness(12, 2, 12, 3), CornerRadius = new CornerRadius(13), Visibility = Visibility.Collapsed };
        ToolTipService.SetToolTip(done, "Move its screens together again");
        Canvas.SetZIndex(done, 4);
        var island = new Island(computer, hull, chip, done);
        done.Click += (_, _) => Adjust(island.Owner, false);
        AttachDrag(chip, island, () => island.Screens[0], () => true);
        Arrangement.Children.Add(hull);
        Arrangement.Children.Add(chip);
        Arrangement.Children.Add(done);
        return island;
    }

    private Screen NewScreen(JsonObject monitor, Island island)
    {
        var face = new Grid { CornerRadius = new CornerRadius(5) };
        if (Tint(island.Computer.Mark) is Color tint)
        {
            var gradient = new LinearGradientBrush { StartPoint = new Point(0, 0), EndPoint = new Point(0, 1) };
            gradient.GradientStops.Add(new GradientStop { Color = WithAlpha(tint, 0.26), Offset = 0 });
            gradient.GradientStops.Add(new GradientStop { Color = WithAlpha(tint, 0.08), Offset = 1 });
            face.Background = gradient;
        }
        var tile = new Border { CornerRadius = new CornerRadius(6), BorderThickness = new Thickness(1), Background = Brush("SolidBackgroundFillColorTertiaryBrush"), Child = face };
        // A paired computer that cannot take input now shows faded.
        if (island.Computer.Peer is JsonObject peer && !Flag(peer["connected"])) tile.Opacity = 0.62;
        var screen = new Screen(monitor, island.Owner, tile);
        var rect = RectOf(monitor, fit!);
        if (monitor["display"] is JsonObject display)
        {
            // A screen too small for its name shows none.
            if (rect.Width >= 64 && rect.Height >= 30)
                face.Children.Add(new Border
                {
                    Background = Brush("SolidBackgroundFillColorTertiaryBrush"), CornerRadius = new CornerRadius(4), Padding = new Thickness(5, 1, 5, 2), Margin = new Thickness(5),
                    HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Bottom,
                    Child = new TextBlock { Text = Text(display["name"], Text(monitor["label"])), FontSize = 11, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, TextTrimming = TextTrimming.CharacterEllipsis, MaxWidth = Math.Max(rect.Width - 22, 1) },
                });
        }
        else if (Math.Min(rect.Width, rect.Height) >= 24)
        {
            // A computer that has not said what screens it has.
            face.Children.Add(new FontIcon { Glyph = island.Owner == Local ? "\uE7F8" : "\uE7F4", FontSize = Math.Min(22, rect.Height * 0.35), Foreground = Brush("TextFillColorSecondaryBrush") });
        }
        ToolTipService.SetToolTip(tile, Text(monitor["label"]));
        AutomationProperties.SetName(tile, $"{ScreenName(monitor)}, {island.Computer.Name}");
        // Out of adjusting, a screen carries its whole computer.
        AttachDrag(tile, island, () => screen, () => adjusting != island.Owner);
        Arrangement.Children.Add(tile);
        return screen;
    }

    // Places the screens, outlines, names and seams; a drag moves its screens
    // to where the engine says they land, else under the pointer.
    private void PlaceArrangement()
    {
        if (fit is not Fit f) return;
        var current = drag;
        var (shiftX, shiftY) = current is null ? (0d, 0d) : current.Landing is JsonObject landing
            ? ((Number(landing["x"]) - current.AnchorX) * f.Scale, (Number(landing["y"]) - current.AnchorY) * f.Scale)
            : (current.Dx, current.Dy);
        bool refused = current is { Asked: not null, Landing: null };
        foreach (var screen in screens.Values)
        {
            screen.Home = RectOf(screen.Monitor, f);
            bool moving = current is not null && current.Moves(screen);
            screen.Shown = moving ? RectOf(screen.Monitor, f, shiftX, shiftY) : screen.Home;
            Canvas.SetLeft(screen.Tile, screen.Shown.X); Canvas.SetTop(screen.Tile, screen.Shown.Y);
            screen.Tile.Width = screen.Shown.Width; screen.Tile.Height = screen.Shown.Height;
            Canvas.SetZIndex(screen.Tile, moving ? 2 : 1);
            screen.Tile.BorderBrush = Brush(moving && refused ? "SystemFillColorCriticalBrush" : moving ? "ControlStrongStrokeColorDefaultBrush" : "CardStrokeColorDefaultBrush");
            screen.Tile.BorderThickness = new Thickness(moving && refused ? 2 : 1);
        }
        var accent = Accent;
        foreach (var island in islands)
        {
            var hull = Hull(island);
            Canvas.SetLeft(island.Hull, hull.X); Canvas.SetTop(island.Hull, hull.Y);
            island.Hull.Width = hull.Width; island.Hull.Height = hull.Height;
            bool selected = selection == island.Owner;
            var tint = Tint(island.Computer.Mark) ?? Color.FromArgb(255, 128, 128, 128);
            island.Hull.Fill = new SolidColorBrush(WithAlpha(tint, 0.1));
            island.Hull.Stroke = new SolidColorBrush(selected ? accent : WithAlpha(tint, 0.5));
            island.Hull.StrokeThickness = selected ? 2.5 : 1.5;
            island.Hull.StrokeDashArray = adjusting == island.Owner ? [4, 3] : [];
            island.Chip.BorderBrush = selected ? new SolidColorBrush(accent) : Brush("CardStrokeColorDefaultBrush");
            island.Chip.BorderThickness = new Thickness(selected ? 2 : 1);
            island.Done.Visibility = adjusting == island.Owner ? Visibility.Visible : Visibility.Collapsed;
        }
        PlaceChips();
        DrawSeams();
    }

    // An island's screens as shown, with room around them.
    private static Rect Hull(Island island)
    {
        var rects = island.Screens.Select(s => s.Shown).ToList();
        double left = rects.Min(r => r.Left), top = rects.Min(r => r.Top);
        double right = rects.Max(r => r.Right), bottom = rects.Max(r => r.Bottom);
        return new Rect(left - HullGap, top - HullGap, right - left + 2 * HullGap, bottom - top + 2 * HullGap);
    }

    // A computer's name sits over its outline's top left corner, or at
    // another corner or just inside, wherever it covers the least of the
    // other computers and names.
    private void PlaceChips()
    {
        double width = Arrangement.ActualWidth, height = Arrangement.Height;
        var hulls = islands.ToDictionary(island => island, Hull);
        var taken = new List<Rect>();
        static double Covered(Rect a, Rect b) => Math.Max(0, Math.Min(a.Right, b.Right) - Math.Max(a.Left, b.Left)) * Math.Max(0, Math.Min(a.Bottom, b.Bottom) - Math.Max(a.Top, b.Top));
        foreach (var island in islands)
        {
            var hull = hulls[island];
            island.Chip.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
            var chip = island.Chip.DesiredSize;
            var done = new Size();
            if (island.Done.Visibility == Visibility.Visible)
            {
                island.Done.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
                done = island.Done.DesiredSize;
            }
            double span = chip.Width + (done.Width > 0 ? ChipGap + done.Width : 0);
            double OnBoard(double x) => Math.Round(Math.Max(4, Math.Min(x, width - span - 4)));
            double start = OnBoard(hull.Left), end = OnBoard(hull.Right - span);
            double above = hull.Top - chip.Height - ChipGap, below = hull.Bottom + ChipGap, inside = hull.Top + HullGap + 4;
            (double X, double Y)[] spots = [(start, above), (end, above), (start, below), (end, below), (start, inside)];
            var others = islands.Where(other => other != island).Select(other => hulls[other]).Concat(taken).ToList();
            // Off the board counts as covering everything.
            double Cost((double X, double Y) spot) => spot.Y < 4 || spot.Y + chip.Height > height - 4 ? double.PositiveInfinity
                : others.Sum(other => Covered(new Rect(spot.X, spot.Y, span, chip.Height), other));
            if (drag is null || island.Spot is null)
            {
                var costs = spots.Select(Cost).ToList();
                island.Spot = costs.IndexOf(costs.Min());
            }
            var (x, wanted) = spots[island.Spot.Value];
            double y = Math.Round(Math.Max(4, Math.Min(wanted, height - chip.Height - 4)));
            taken.Add(new Rect(x, y, span, chip.Height));
            Canvas.SetLeft(island.Chip, x); Canvas.SetTop(island.Chip, y);
            Canvas.SetLeft(island.Done, x + chip.Width + ChipGap); Canvas.SetTop(island.Done, y + Math.Round((chip.Height - done.Height) / 2));
        }
    }

    // Lit seams where the pointer crosses to another computer, and dotted
    // joints between one computer's own screens. While a drag waits for the
    // engine, only the seams it cannot change show.
    private void DrawSeams()
    {
        foreach (var mark in seamMarks) Arrangement.Children.Remove(mark);
        seamMarks.Clear();
        if (fit is not Fit f) return;
        IEnumerable<JsonObject> seams = (state?["seams"] as JsonArray)?.OfType<JsonObject>() ?? [];
        if (drag?.Landing is JsonObject landing) seams = (landing["seams"] as JsonArray)?.OfType<JsonObject>() ?? [];
        else if (drag is Drag current)
        {
            var moving = screens.Values.Where(current.Moves).Select(s => s.Id).ToHashSet();
            seams = seams.Where(s => !moving.Contains(Text(s["a"])) && !moving.Contains(Text(s["b"])));
        }
        // Seams fade while sharing is off, since the pointer crosses none.
        double strength = Flag(state?["sharing"]) ? 1 : 0.3;
        var accent = Accent;
        void Add(UIElement mark) { Canvas.SetZIndex(mark, 3); seamMarks.Add(mark); Arrangement.Children.Add(mark); }
        Line Stroke((Point A, Point B) ends, Brush brush, double thickness)
        {
            var line = new Line { X1 = ends.A.X, Y1 = ends.A.Y, X2 = ends.B.X, Y2 = ends.B.Y, Stroke = brush, StrokeThickness = thickness, IsHitTestVisible = false, StrokeStartLineCap = PenLineCap.Round, StrokeEndLineCap = PenLineCap.Round, StrokeDashCap = PenLineCap.Round };
            Add(line);
            return line;
        }
        foreach (var seam in seams)
        {
            bool vertical = Flag(seam["vertical"]);
            if (Flag(seam["same_computer"]))
            {
                Stroke(SeamLine(seam, f, 4), Brush("TextFillColorTertiaryBrush"), 2).StrokeDashArray = [0.05, 2.5];
                continue;
            }
            var ends = SeamLine(seam, f, 3);
            Stroke(ends, new SolidColorBrush(WithAlpha(accent, 0.28 * strength)), 11);
            Stroke(ends, new SolidColorBrush(WithAlpha(accent, strength)), 4);
            // Arrows across it, where there is room.
            if ((Number(seam["end"]) - Number(seam["start"])) * f.Scale <= 34) continue;
            var badge = new Grid { Width = 18, Height = 18, IsHitTestVisible = false };
            badge.Children.Add(new Ellipse { Fill = new SolidColorBrush(WithAlpha(accent, strength)) });
            badge.Children.Add(new FontIcon { Glyph = "\uE8AB", FontSize = 10, Foreground = new SolidColorBrush(Microsoft.UI.Colors.White), RenderTransformOrigin = new Point(0.5, 0.5), RenderTransform = new RotateTransform { Angle = vertical ? 0 : 90 } });
            Canvas.SetLeft(badge, (ends.A.X + ends.B.X) / 2 - 9); Canvas.SetTop(badge, (ends.A.Y + ends.B.Y) / 2 - 9);
            Add(badge);
        }
    }

    // A seam on the board, short of its ends by `inset`.
    private static (Point A, Point B) SeamLine(JsonObject seam, Fit f, double inset)
    {
        bool vertical = Flag(seam["vertical"]);
        int at = Number(seam["at"]), start = Number(seam["start"]), end = Number(seam["end"]);
        double across = Math.Round(vertical ? f.OffsetX + (at - f.Left) * f.Scale : f.OffsetY + (at - f.Top) * f.Scale);
        double along = vertical ? f.OffsetY - f.Top * f.Scale : f.OffsetX - f.Left * f.Scale;
        double from = along + start * f.Scale + inset, to = Math.Max(from, along + end * f.Scale - inset);
        return vertical ? (new Point(across, from), new Point(across, to)) : (new Point(from, across), new Point(to, across));
    }

    // A press on a name or a screen picks up its computer, or the screen
    // alone while the computer is being adjusted. Past a click, the screens
    // follow the pointer; a click selects the computer.
    private void AttachDrag(FrameworkElement element, Island island, Func<Screen> anchor, Func<bool> computer)
    {
        element.PointerPressed += (_, e) =>
        {
            var point = e.GetCurrentPoint(Arrangement);
            if (applying || drag is not null || !point.Properties.IsLeftButtonPressed) return;
            pressed = new Press(island, anchor(), computer(), point.Position, e.Pointer.PointerId);
            element.CapturePointer(e.Pointer);
            Board.Focus(FocusState.Pointer);
            e.Handled = true;
        };
        element.PointerMoved += (_, e) =>
        {
            if (pressed is not Press press || press.Pointer != e.Pointer.PointerId || fit is null) return;
            var at = e.GetCurrentPoint(Arrangement).Position;
            double dx = at.X - press.Start.X, dy = at.Y - press.Start.Y;
            if (drag is null)
            {
                if (Math.Abs(dx) + Math.Abs(dy) < 3) return;
                drag = new Drag(press.Island.Owner, press.Anchor, press.Computer, fit);
                dragging = true;
                Select(press.Island.Owner);
            }
            if (drag is not { Dropped: false } current) return;
            (current.Dx, current.Dy) = Travel(current, dx, dy);
            PlaceArrangement();
            Ask();
        };
        element.PointerReleased += async (_, e) =>
        {
            if (pressed is not Press press || press.Pointer != e.Pointer.PointerId) return;
            pressed = null;
            element.ReleasePointerCaptures();
            e.Handled = true;
            if (drag is { Dropped: false } current) await FinishMoveAsync(current);
            else Select(press.Island.Owner);
        };
        // Lost without a release, as when the window loses focus: the screens go back.
        element.PointerCaptureLost += (_, _) =>
        {
            if (pressed is null) return;
            pressed = null;
            if (drag is not { Dropped: false }) return;
            drag = null; dragging = false;
            PlaceArrangement();
        };
        element.DoubleTapped += (_, e) => { Adjust(island.Owner, adjusting != island.Owner); e.Handled = true; };
    }

    // As far as the pointer went, but with the screens kept on the board.
    private (double, double) Travel(Drag current, double dx, double dy)
    {
        var moving = screens.Values.Where(current.Moves).Select(s => s.Home).ToList();
        if (moving.Count == 0) return (dx, dy);
        double left = moving.Min(r => r.Left), top = moving.Min(r => r.Top);
        double right = moving.Max(r => r.Right), bottom = moving.Max(r => r.Bottom);
        return (Math.Clamp(dx, -left, Math.Max(-left, Arrangement.ActualWidth - right)), Math.Clamp(dy, -top, Math.Max(-top, Arrangement.Height - bottom)));
    }

    // Asks the engine where the drag lands, one question at a time, the newest travel next.
    private async void Ask()
    {
        if (drag is not { Dropped: false } current || asking) return;
        var (x, y) = current.Target;
        string asked = $"{x},{y}";
        if (current.Asked == asked) return;
        asking = true;
        JsonNode? landing;
        try { landing = await engine.RequestAsync(new { command = "preview", id = current.AnchorId, x, y, tolerance = current.Tolerance, computer = current.Computer }, 1500); }
        // Without an answer, the screens follow the pointer.
        catch (Exception) { return; }
        finally { asking = false; }
        if (drag == current && !current.Dropped)
        {
            current.Asked = asked;
            current.Landing = landing as JsonObject;
            PlaceArrangement();
        }
        Ask();
    }

    // The screens stay where they show until the engine's state moves them,
    // or go back if it refuses.
    private async Task FinishMoveAsync(Drag current)
    {
        current.Dropped = true;
        dragging = false;
        var (x, y) = current.Target;
        PlaceArrangement();
        await SendAsync(new { command = "move_tile", id = current.AnchorId, x, y, tolerance = current.Tolerance, computer = current.Computer });
        if (drag != current) return;
        drag = null;
        PlaceArrangement();
    }

    private void Select(string? owner)
    {
        if (owner == selection) return;
        selection = owner;
        if (adjusting != owner) adjusting = null;
        PlaceArrangement();
        DrawInspector();
    }

    // Whether a computer's screens move one by one instead of together.
    private void Adjust(string owner, bool on, bool redraw = true)
    {
        bool reselect = selection != owner;
        selection = owner;
        adjusting = on ? owner : null;
        PlaceArrangement();
        if (redraw || reselect) DrawInspector();
    }

    private async void Board_KeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key == VirtualKey.Escape && selection is not null) { Select(null); e.Handled = true; return; }
        (int dx, int dy) = e.Key switch { VirtualKey.Left => (-1, 0), VirtualKey.Right => (1, 0), VirtualKey.Up => (0, -1), VirtualKey.Down => (0, 1), _ => (0, 0) };
        if ((dx, dy) == (0, 0) || drag is not null || islands.Count == 0) return;
        e.Handled = true;
        // The first arrow picks a computer to move.
        if (islands.Find(i => i.Owner == selection) is not Island island) { Select(islands[0].Owner); return; }
        var anchor = island.Screens[0].Monitor;
        await SendAsync(new { command = "move_tile", id = Text(anchor["id"]), x = Number(anchor["x"]) + dx * KeyStep, y = Number(anchor["y"]) + dy * KeyStep, tolerance = KeyTolerance, computer = true });
    }

    // A click beside the computers lets go of the selected one.
    private void Arrangement_Tapped(object sender, TappedRoutedEventArgs e)
    {
        if (ReferenceEquals(e.OriginalSource, Arrangement)) Select(null);
    }

    private TextBlock Heading(string text) => new() { Text = text, Style = (Style)Root.Resources["SectionHeading"], Margin = new Thickness(0, 16, 0, 4) };

    private void DrawInspector()
    {
        PeersPanel.Children.Clear();
        var computers = Computers();
        var selected = computers.Find(c => c.Owner == selection);
        if (selected is null && selection is not null)
        {
            // A removed computer lets go of the selection.
            selection = adjusting = null;
            PlaceArrangement();
        }
        if (selected is null) DrawComputers(computers);
        else DrawComputer(selected);
    }

    private void DrawComputers(List<Computer> computers)
    {
        foreach (var computer in computers)
        {
            var content = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10, VerticalAlignment = VerticalAlignment.Center };
            if (computer.Peer is not null) content.Children.Add(Dot(computer.Peer));
            if (MarkView(computer.Mark) is { } mark) content.Children.Add(mark);
            var card = new SettingsCard
            {
                Header = computer.Name, Description = Summary(computer, computer.Peer is null ? null : PeerState(computer.Peer).Label), Content = content, IsClickEnabled = true,
                HeaderIcon = new FontIcon { Glyph = computer.Owner == Local ? "\uE7F8" : "\uE7F4", Foreground = Tint(computer.Mark) is Color tint ? new SolidColorBrush(tint) : Brush("TextFillColorSecondaryBrush") },
            };
            card.Click += (_, _) => { Select(computer.Owner); Board.Focus(FocusState.Programmatic); };
            PeersPanel.Children.Add(card);
        }
        if (!computers.Exists(c => c.Peer is not null))
            PeersPanel.Children.Add(new SettingsCard { Header = "No computers added", Description = "Find a nearby computer or add one by its Tailscale name or address.", HeaderIcon = new FontIcon { Glyph = "\uE7F4" } });
    }

    private void DrawComputer(Computer computer)
    {
        var peer = computer.Peer;
        string name = computer.Name;
        bool local = computer.Owner == Local;
        // Its name, state and mark, with a way back to every computer.
        var header = new Grid { ColumnSpacing = 12, Margin = new Thickness(0, 0, 0, 4) };
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        header.ColumnDefinitions.Add(new ColumnDefinition());
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        header.Children.Add(new FontIcon { Glyph = local ? "\uE7F8" : "\uE7F4", FontSize = 28, Foreground = Tint(computer.Mark) is Color tint ? new SolidColorBrush(tint) : Brush("TextFillColorSecondaryBrush") });
        var title = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
        title.Children.Add(new TextBlock { Text = name, FontSize = 20, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, TextTrimming = TextTrimming.CharacterEllipsis });
        var status = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        if (peer is not null) status.Children.Add(Dot(peer));
        status.Children.Add(new TextBlock { Text = peer is null ? "This computer" : PeerState(peer).Label, Foreground = Brush("TextFillColorSecondaryBrush"), TextWrapping = TextWrapping.Wrap, IsTextSelectionEnabled = true });
        title.Children.Add(status);
        Grid.SetColumn(title, 1);
        header.Children.Add(title);
        var end = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 12, VerticalAlignment = VerticalAlignment.Center };
        if (MarkView(computer.Mark) is { } mark) end.Children.Add(mark);
        var close = new Button { Content = new FontIcon { Glyph = "\uE711", FontSize = 14 }, Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent), BorderThickness = new Thickness(0) };
        ToolTipService.SetToolTip(close, "Show all computers");
        AutomationProperties.SetName(close, "Show all computers");
        close.Click += (_, _) => Select(null);
        end.Children.Add(close);
        Grid.SetColumn(end, 2);
        header.Children.Add(end);
        PeersPanel.Children.Add(header);

        if (computer.Monitors.Count > 1)
        {
            var adjust = new ToggleSwitch { IsOn = adjusting == computer.Owner };
            AutomationProperties.SetName(adjust, "Adjust screens");
            adjust.Toggled += (_, _) => Adjust(computer.Owner, adjust.IsOn, redraw: false);
            PeersPanel.Children.Add(new SettingsCard { Header = "Adjust screens", Description = "Move its screens one by one instead of together. Double-clicking it on the arrangement does too.", HeaderIcon = new FontIcon { Glyph = "\uE7C4" }, Content = adjust });
        }
        if (local)
        {
            var displays = new SettingsCard { Header = "Display settings", Description = "Arrange this PC’s own screens in Windows Settings.", HeaderIcon = new FontIcon { Glyph = "\uE7F4" }, IsClickEnabled = true, ActionIcon = new FontIcon { Glyph = "\uE8A7" } };
            displays.Click += async (_, _) =>
            {
                try { await Launcher.LaunchUriAsync(new Uri("ms-settings:display")); }
                catch (Exception error) { ShowError(error); }
            };
            PeersPanel.Children.Add(displays);
        }
        if (peer is not null)
        {
            var control = new Button { Content = "Control", IsEnabled = Flag(peer["connected"]) };
            AutomationProperties.SetName(control, "Control " + name);
            control.Click += async (_, _) => await SendAsync(new { command = "activate", peer = name });
            PeersPanel.Children.Add(new SettingsCard { Header = "Take control", Description = "Move input to it now. Crossing an edge does too.", HeaderIcon = new FontIcon { Glyph = "\uE962" }, Content = control });
        }

        // Where the pointer goes from this computer's screens to another's.
        var monitors = ActiveMonitors();
        var mine = computer.Monitors.Select(m => Text(m["id"])).ToHashSet();
        var crossings = (state?["seams"] as JsonArray)?.OfType<JsonObject>()
            .Where(s => !Flag(s["same_computer"]) && mine.Contains(Text(s["a"])) != mine.Contains(Text(s["b"]))).ToList() ?? [];
        if (computer.Monitors.Count > 0)
        {
            PeersPanel.Children.Add(Heading("Crossings"));
            if (local && computer.Monitors.Count > 1)
                PeersPanel.Children.Add(new TextBlock { Text = "Between this PC’s own screens, the pointer follows Windows display settings.", Style = (Style)Root.Resources["SecondaryText"], Margin = new Thickness(0, 0, 0, 4) });
            if (crossings.Count == 0)
                PeersPanel.Children.Add(new SettingsCard { Header = "None yet", Description = "Move it until an edge touches another computer’s screen.", HeaderIcon = new FontIcon { Glyph = "\uE8AB" } });
            foreach (var seam in crossings)
            {
                string a = Text(seam["a"]), b = Text(seam["b"]);
                string here = mine.Contains(a) ? a : b, there = here == a ? b : a;
                bool vertical = Flag(seam["vertical"]);
                string side = vertical ? (here == a ? "Right" : "Left") : (here == a ? "Bottom" : "Top");
                var other = monitors.Find(m => Text(m["id"]) == there);
                string owner = other is null ? there : other["peer"] is JsonValue p ? p.GetValue<string>() : ThisPc;
                PeersPanel.Children.Add(new SettingsCard
                {
                    Header = owner, Description = $"{side} edge of {ScreenName(monitors.Find(m => Text(m["id"]) == here))} → {ScreenName(other)}",
                    HeaderIcon = new FontIcon { Glyph = "\uE8AB", Foreground = Brush("AccentTextFillColorPrimaryBrush"), RenderTransformOrigin = new Point(0.5, 0.5), RenderTransform = new RotateTransform { Angle = vertical ? 0 : 90 } },
                });
            }
            PeersPanel.Children.Add(Heading("Screens"));
            foreach (var monitor in computer.Monitors)
                PeersPanel.Children.Add(new SettingsCard { Header = ScreenName(monitor), Description = ScreenDetail(monitor["display"]), HeaderIcon = new FontIcon { Glyph = "\uE7F4" } });
        }
        if (peer is null) return;

        PeersPanel.Children.Add(Heading("Settings"));
        var mode = new ComboBox { MinWidth = 170, ItemsSource = new[] { "Standard keys", "PC key positions", "Mac shortcuts" }, SelectedIndex = Text(peer["keyboard"]) switch { "pc_positions" => 1, "mac" => 2, _ => 0 } };
        AutomationProperties.SetName(mode, "Keyboard mode for " + name);
        mode.SelectionChanged += async (_, _) => { if (!applying && mode.SelectedIndex >= 0) await SendAsync(new { command = "keyboard", peer = name, mode = new[] { "standard", "pc_positions", "mac" }[mode.SelectedIndex] }); };
        PeersPanel.Children.Add(new SettingsCard { Header = "Keyboard mode", Description = "How keys from this computer work here.", HeaderIcon = new FontIcon { Glyph = "\uE765" }, Content = mode });
        var scroll = new ToggleSwitch { IsOn = Flag(peer["reverse_scroll"]) };
        AutomationProperties.SetName(scroll, "Reverse scrolling for " + name);
        scroll.Toggled += async (_, _) => { if (!applying) await SendAsync(new { command = "reverse_scroll", peer = name, enabled = scroll.IsOn }); };
        PeersPanel.Children.Add(new SettingsCard { Header = "Reverse scrolling", HeaderIcon = new FontIcon { Glyph = "\uE8CB" }, Content = scroll });
        var forget = new Button { Content = "Remove" };
        AutomationProperties.SetName(forget, "Remove " + name);
        forget.Click += async (_, _) =>
        {
            var dialog = new ContentDialog { XamlRoot = Root.XamlRoot, Title = "Remove " + name + "?", Content = "You’ll need to add this computer again to share input.", PrimaryButtonText = "Remove", CloseButtonText = "Cancel", DefaultButton = ContentDialogButton.Close };
            if (await dialog.ShowAsync() == ContentDialogResult.Primary) await SendAsync(new { command = "forget", peer = name });
        };
        PeersPanel.Children.Add(new SettingsCard { Header = "Remove computer", Description = "Stop sharing input with it until it is added again.", HeaderIcon = new FontIcon { Glyph = "\uE74D" }, Content = forget });
    }
}
