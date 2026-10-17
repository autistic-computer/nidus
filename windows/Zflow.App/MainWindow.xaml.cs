using System.Diagnostics;
using System.Text.Json.Nodes;
using System.Runtime.InteropServices.WindowsRuntime;
using CommunityToolkit.WinUI.Controls;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.Win32;
using Windows.Foundation;

namespace Zflow;
public sealed partial class MainWindow : Window
{
    private readonly EngineClient engine = new();
    private readonly DispatcherTimer timer = new() { Interval = TimeSpan.FromSeconds(2) };
    private readonly NativeWindow native;
    private readonly AppUpdates updates = new();
    private readonly DispatcherTimer updateTimer = new() { Interval = TimeSpan.FromHours(6) };
    private JsonObject? state;
    private bool refreshing, applying, dragging, quitting;
    private string previousView = "";
    public MainWindow()
    {
        InitializeComponent();
        Navigation.SelectedItem = ComputersNav;
        VersionText.Text = typeof(App).Assembly.GetName().Version?.ToString(3) ?? "";
        MessageBar.Closed += (_, _) => MessageBar.Visibility = Visibility.Collapsed;
        SystemBackdrop = new MicaBackdrop();
        ExtendsContentIntoTitleBar = true; SetTitleBar(TitleArea);
        string iconPath = Path.Combine(AppContext.BaseDirectory, "Assets", "zflow.ico");
        AppWindow.SetIcon(iconPath);
        native = new NativeWindow(this, iconPath);
        native.QuitRequested += async () => await QuitAsync();
        AppWindow.Closing += (_, args) => { if (!quitting) { args.Cancel = true; AppWindow.Hide(); } };
        timer.Tick += async (_, _) => await RefreshAsync();
        updateTimer.Tick += async (_, _) => { if (AutoUpdatesSwitch.IsOn) await CheckUpdatesAsync(); };
        Root.Loaded += async (_, _) =>
        {
            // AppWindow uses physical pixels; size the settings UI consistently at any DPI.
            double dpi = Root.XamlRoot.RasterizationScale;
            var area = Microsoft.UI.Windowing.DisplayArea.GetFromWindowId(AppWindow.Id, Microsoft.UI.Windowing.DisplayAreaFallback.Primary).WorkArea;
            int width = Math.Min((int)(1120 * dpi), area.Width - 48), height = Math.Min((int)(820 * dpi), area.Height - 48);
            AppWindow.MoveAndResize(new Windows.Graphics.RectInt32(area.X + (area.Width - width) / 2, area.Y + (area.Height - height) / 2, width, height));
            applying = true;
            using (var run = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Run")) StartupSwitch.IsOn = run?.GetValue("zflow") is not null;
            using (var preferences = Registry.CurrentUser.OpenSubKey(@"Software\zflow")) AutoUpdatesSwitch.IsOn = preferences?.GetValue("CheckForUpdates") is not int enabled || enabled != 0;
            AutoUpdatesSwitch.IsEnabled = updates.IsEnabled;
            applying = false;
            await StartAsync(); timer.Start();
            UpdateUpdateControls();
            if (updates.IsEnabled) { updateTimer.Start(); if (AutoUpdatesSwitch.IsOn) _ = CheckUpdatesAsync(); }
            if (Environment.GetCommandLineArgs().Contains("--background")) AppWindow.Hide();
            var args = Environment.GetCommandLineArgs();
            int capture = Array.IndexOf(args, "--capture-preview");
            if (capture >= 0 && capture + 1 < args.Length) await CapturePreviewAsync(args[capture + 1]);
        };
    }
    private async Task StartAsync() { try { Update(await engine.StartAsync()); } catch (Exception e) { ShowError(e); } }
    private async Task RefreshAsync()
    {
        if (refreshing || dragging || applying || quitting) return;
        refreshing = true;
        try { Update(await engine.SendAsync(new { command = "status" }, 1500)); }
        catch (Exception e) { ConnectionSummary.Text = "Input engine is not responding"; SharingSwitch.IsEnabled = false; ShowError(e); }
        finally { refreshing = false; }
    }
    private async Task SendAsync(object request)
    {
        if (applying) return;
        applying = true;
        try { Update(await engine.SendAsync(request)); MessageBar.IsOpen = false; }
        catch (Exception e) { ShowError(e); }
        finally { applying = false; }
    }
    private void ShowError(Exception e) { MessageBar.Title = "zflow needs attention"; MessageBar.Message = e is OperationCanceledException ? "The engine did not answer. Restart it under Settings → Connection and troubleshooting." : e.Message; MessageBar.Visibility = Visibility.Visible; MessageBar.IsOpen = true; }
    private static string Text(JsonNode? n, string fallback = "") => n?.GetValue<string>() ?? fallback;
    private static bool Flag(JsonNode? n) => n?.GetValue<bool>() ?? false;
    private static int Number(JsonNode? n) => n?.GetValue<int>() ?? 0;
    private void Update(JsonObject next)
    {
        state = next;
        bool wasApplying = applying; applying = true;
        try
        {
            SharingSwitch.IsEnabled = true; SharingSwitch.IsOn = Flag(next["sharing"]);
            bool elevated = Flag(next["elevated"]);
            ElevationCard.Header = elevated ? "Running as administrator" : "Running as user";
            ElevationCard.Description = elevated ? "Administrator windows can receive input. Windows sign-in and UAC prompts still require local input." : "Administrator windows need zflow to run as administrator too. Startup runs normally.";
            ElevateButton.Visibility = elevated ? Visibility.Collapsed : Visibility.Visible;
            ClipboardSwitch.IsOn = Flag(next["clipboard"]); PauseEdgesSwitch.IsOn = Flag(next["pause_at_edges"]);
            LocalName.Text = Text(next["name"], "This computer"); LocalMark.Text = "Mark  " + Text(next["mark"]);
            string sending = Text(next["sending"]), receiving = Text(next["receiving"]);
            int connected = next["peers"]?.AsArray().Count(p => Flag(p?["connected"])) ?? 0;
            ConnectionSummary.Text = sending != "" ? $"Controlling {sending}" : receiving != "" ? $"Controlled by {receiving}" : !Flag(next["available"]) ? "Waiting for an unlocked Windows desktop" : !SharingSwitch.IsOn ? "Sharing paused · your input stays here" : $"Ready to share · {connected} computer{(connected == 1 ? "" : "s")} connected";
            NetworkDetails.Text = "Listening on " + Text(next["listen"]) + "\n" + string.Join("\n", next["addresses"]?.AsArray().Select(a => Text(a)) ?? []);
            // What the arrangement and the computers under it show.
            var shown = new JsonObject();
            foreach (string key in new[] { "peers", "nearby", "layout", "seams", "sending", "receiving", "mark", "sharing" }) shown[key] = next[key]?.DeepClone();
            string view = shown.ToJsonString();
            if (view != previousView && !dragging) { previousView = view; DrawArrangement(); DrawInspector(); DrawNearby(); }
            if (next["notice"] is JsonValue notice) { MessageBar.Title = "Connection notice"; MessageBar.Message = notice.GetValue<string>(); MessageBar.Visibility = Visibility.Visible; MessageBar.IsOpen = true; }
        }
        finally { applying = wasApplying; }
    }
    private static Brush Brush(string key) => (Brush)Application.Current.Resources[key];
    private void DrawNearby()
    {
        NearbyPanel.Children.Clear();
        bool found = state?["nearby"] is JsonArray { Count: > 0 };
        NearbySection.Visibility = found || state?["peers"] is not JsonArray { Count: > 0 } ? Visibility.Visible : Visibility.Collapsed;
        if (state?["nearby"] is not JsonArray nearby || nearby.Count == 0) { NearbyPanel.Children.Add(new TextBlock { Text = "Looking for computers running zflow…", TextWrapping = TextWrapping.Wrap, Foreground = Brush("TextFillColorSecondaryBrush") }); return; }
        foreach (var peer in nearby)
        {
            string details = Text(peer?["state"]) switch {
                "different_version" => "Different zflow version. Update both computers.",
                "identifying" => "Waiting for this computer to identify itself…",
                "duplicate_name" => "Same name as another computer · compare mark " + Text(peer?["mark"]),
                _ => Text(peer?["os"]) + " · Mark " + Text(peer?["mark"])
            };
            string id = Text(peer?["id"]); var add = new Button { Content = "Add computer", IsEnabled = Text(peer?["state"]) is "ready" or "duplicate_name" };
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(add, "Add " + Text(peer?["name"]));
            add.Click += async (_, _) => await SendAsync(new { command = "trust", key = id });
            NearbyPanel.Children.Add(new SettingsCard { Header = Text(peer?["name"]), Description = details, HeaderIcon = new FontIcon { Glyph = "\uE7F4" }, Content = add });
        }
    }
    private async void AddAddress_Click(object sender, RoutedEventArgs e)
    {
        var address = new TextBox { PlaceholderText = "macbook or 100.x.x.x", Header = "Computer name or IP address" };
        var content = new StackPanel { Spacing = 16 };
        content.Children.Add(new TextBlock { Text = "Enter a local or Tailscale address. Make sure zflow is running on the other computer.", TextWrapping = TextWrapping.Wrap });
        content.Children.Add(address);
        var dialog = new ContentDialog { XamlRoot = Root.XamlRoot, Title = "Add computer", Content = content, PrimaryButtonText = "Find computer", CloseButtonText = "Cancel", DefaultButton = ContentDialogButton.Primary, IsPrimaryButtonEnabled = false };
        address.TextChanged += (_, _) => dialog.IsPrimaryButtonEnabled = !string.IsNullOrWhiteSpace(address.Text);
        if (await dialog.ShowAsync() == ContentDialogResult.Primary && !string.IsNullOrWhiteSpace(address.Text)) await SendAsync(new { command = "nearby", address = address.Text.Trim() });
    }
    private async void Sharing_Toggled(object sender, RoutedEventArgs e) { if (!applying && state is not null) await SendAsync(new { command = "sharing", enabled = SharingSwitch.IsOn }); }
    private async void Clipboard_Toggled(object sender, RoutedEventArgs e) { if (!applying && state is not null) await SendAsync(new { command = "clipboard", enabled = ClipboardSwitch.IsOn }); }
    private async void PauseEdges_Toggled(object sender, RoutedEventArgs e) { if (!applying && state is not null) await SendAsync(new { command = "pause_at_edges", enabled = PauseEdgesSwitch.IsOn }); }
    private async void Local_Click(object sender, RoutedEventArgs e) => await SendAsync(new { command = "local" });
    private void Navigation_SelectionChanged(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        if (args.SelectedItem is not NavigationViewItem item || ComputersPage is null) return;
        string page = item.Tag as string ?? "computers";
        ComputersPage.Visibility = page == "computers" ? Visibility.Visible : Visibility.Collapsed;
        SettingsPage.Visibility = page == "settings" ? Visibility.Visible : Visibility.Collapsed;
        AboutPage.Visibility = page == "about" ? Visibility.Visible : Visibility.Collapsed;
        PageTitle.Text = item.Content.ToString();
        PageScroll.ChangeView(null, 0, null, true);
        if (page == "computers") DrawArrangement();
    }
    private async void OpenLink_Click(object sender, RoutedEventArgs e)
    {
        try { if (sender is FrameworkElement { Tag: string url }) await Windows.System.Launcher.LaunchUriAsync(new Uri(url)); }
        catch (Exception error) { ShowError(error); }
    }
    private void Arrangement_SizeChanged(object sender, SizeChangedEventArgs e) => DrawArrangement();
    private void Startup_Toggled(object sender, RoutedEventArgs e)
    {
        if (applying) return;
        try { using var key = Registry.CurrentUser.CreateSubKey(@"Software\Microsoft\Windows\CurrentVersion\Run"); if (StartupSwitch.IsOn) key.SetValue("zflow", $"\"{Installation.StartupExecutable}\" --background"); else key.DeleteValue("zflow", false); }
        catch (Exception error) { ShowError(error); }
    }
    private void OpenConfig_Click(object sender, RoutedEventArgs e) { try { var start = new ProcessStartInfo("notepad.exe") { UseShellExecute = false }; start.ArgumentList.Add(EngineClient.ConfigPath); Process.Start(start); } catch (Exception error) { ShowError(error); } }
    private async void Restart_Click(object sender, RoutedEventArgs e)
    {
        if (applying || quitting) return;
        applying = true;
        try
        {
            try { await engine.StopAsync(); }
            catch (Exception error) when (error is IOException or OperationCanceledException or TimeoutException) { } // Start also recovers an exited engine.
            await StartAsync();
        }
        catch (Exception error) { ShowError(error); }
        finally { applying = false; }
    }
    private async void Elevate_Click(object sender, RoutedEventArgs e)
    {
        if (applying || quitting) return;
        applying = true;
        try
        {
            if (!await Elevation.RequestRestartAsync()) return;
            // The replacement waits for this process, including the old engine,
            // before acquiring the single-instance mutex or launching input hooks.
            await engine.StopAsync();
            await QuitAsync(engineStopped: true);
        }
        catch (Exception error) { ShowError(error); }
        finally { applying = false; }
    }
    private async void Quit_Click(object sender, RoutedEventArgs e) => await QuitAsync();
    private async Task QuitAsync(bool engineStopped = false) { if (quitting) return; quitting = true; timer.Stop(); updateTimer.Stop(); if (!engineStopped) { try { await engine.StopAsync(); } catch { } } native.Dispose(); Close(); Application.Current.Exit(); }
    private void AutoUpdates_Toggled(object sender, RoutedEventArgs e)
    {
        if (applying || !updates.IsEnabled) return;
        try { using var preferences = Registry.CurrentUser.CreateSubKey(@"Software\zflow"); preferences.SetValue("CheckForUpdates", AutoUpdatesSwitch.IsOn ? 1 : 0); if (AutoUpdatesSwitch.IsOn) _ = CheckUpdatesAsync(); }
        catch (Exception error) { ShowError(error); }
    }
    private void UpdateUpdateControls(string? message = null)
    {
        CheckUpdatesButton.IsEnabled = updates.IsEnabled && !updates.IsBusy;
        InstallUpdateButton.IsEnabled = !updates.IsBusy;
        InstallUpdateButton.Visibility = updates.CanDownload || updates.CanRestart ? Visibility.Visible : Visibility.Collapsed;
        InstallUpdateButton.Content = updates.CanRestart ? "Restart and update" : "Download update";
        AboutNav.InfoBadge = updates.CanDownload || updates.CanRestart ? new InfoBadge() : null;
        GetInstallerLink.Visibility = updates.IsEnabled ? Visibility.Collapsed : Visibility.Visible;
        UpdateStatus.Text = message ?? (!updates.IsEnabled ? "Install zflow with the Windows installer to receive updates. Portable and development copies update manually."
            : updates.CanRestart ? $"zflow {updates.Version} is ready. Restart when you are ready to pause sharing."
            : updates.CanDownload ? $"zflow {updates.Version} is available."
            : "You can check for the latest stable release here.");
    }
    private async Task CheckUpdatesAsync()
    {
        if (!updates.IsEnabled || updates.IsBusy || quitting) return;
        try
        {
            var check = updates.CheckAsync();
            UpdateUpdateControls("Checking for updates…");
            await check;
            UpdateUpdateControls(updates.Version is null ? "You have the latest stable release." : null);
        }
        catch (Exception error) { UpdateUpdateControls("Could not check for updates. " + error.Message); }
    }
    private async void CheckUpdates_Click(object sender, RoutedEventArgs e) => await CheckUpdatesAsync();
    private async void InstallUpdate_Click(object sender, RoutedEventArgs e)
    {
        if (updates.IsBusy || quitting || applying) return;
        if (updates.CanDownload)
        {
            try
            {
                UpdateProgress.Value = 0; UpdateProgress.Visibility = Visibility.Visible;
                var download = updates.DownloadAsync(progress => DispatcherQueue.TryEnqueue(() => UpdateProgress.Value = progress));
                UpdateUpdateControls("Downloading update…");
                await download;
                UpdateUpdateControls();
            }
            catch (Exception error) { UpdateUpdateControls("Could not download the update. " + error.Message); }
            finally { UpdateProgress.Visibility = Visibility.Collapsed; }
            return;
        }
        if (!updates.CanRestart) return;
        var dialog = new ContentDialog { XamlRoot = Root.XamlRoot, Title = "Restart and update zflow?", Content = "Input sharing will stop while zflow updates and restarts. Your settings and paired computers will stay.", PrimaryButtonText = "Restart and update", CloseButtonText = "Later", DefaultButton = ContentDialogButton.Close };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary || quitting) return;
        applying = true;
        timer.Stop(); updateTimer.Stop();
        try
        {
            await updates.PrepareRestartAsync(() => engine.StopAsync());
            await QuitAsync(engineStopped: true);
        }
        catch (Exception error)
        {
            // If shutdown or updater launch fails, resume the existing input engine.
            await StartAsync();
            UpdateUpdateControls("Could not install the update. " + error.Message);
            timer.Start(); updateTimer.Start();
        }
        finally { applying = false; }
    }
    private async Task CapturePreviewAsync(string path)
    {
        try
        {
            // Render the real WinUI visual tree for visual QA on machines whose
            // graphics driver does not expose the composition surface to capture.
            await Task.Delay(500);
            var bitmap = new Microsoft.UI.Xaml.Media.Imaging.RenderTargetBitmap();
            await bitmap.RenderAsync(Root);
            var pixels = (await bitmap.GetPixelsAsync()).ToArray();
            using var stream = File.Create(Path.GetFullPath(path)).AsRandomAccessStream();
            var encoder = await Windows.Graphics.Imaging.BitmapEncoder.CreateAsync(Windows.Graphics.Imaging.BitmapEncoder.PngEncoderId, stream);
            encoder.SetPixelData(Windows.Graphics.Imaging.BitmapPixelFormat.Bgra8, Windows.Graphics.Imaging.BitmapAlphaMode.Premultiplied, (uint)bitmap.PixelWidth, (uint)bitmap.PixelHeight, 96, 96, pixels);
            await encoder.FlushAsync();
        }
        catch (Exception error) { ShowError(error); }
    }
}
