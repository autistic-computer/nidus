import Adw from 'gi://Adw?version=1';
import Cairo from 'cairo';
import Gdk from 'gi://Gdk?version=4.0';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Gtk from 'gi://Gtk?version=4.0';
import Pango from 'gi://Pango';
import {Client, compatible, statusText} from './client.js';

// src/core/keymap.rs KeyboardMode, in dropdown order.
const KEYBOARD_MODES = ['standard', 'pc_positions', 'mac'];
const HEALTH_ICONS = {ok: 'object-select-symbolic', warning: 'dialog-warning-symbolic', error: 'dialog-error-symbolic'};
const NO_LAYOUT = 'Arrange computers in zflow on the other computer.';
const LAYOUT_HINT = 'Drag a computer until its screens touch another’s; the pointer crosses at the lit edges. Double-click one to move its screens one by one.';
// As in the Mac app (ComputerLayout.swift): an arrow key moves a computer
// 100 layout units, about an inch, and snaps within a little less, so a step
// away from a seam is not pulled back to it. A drag snaps within 14 pixels.
const KEY_STEP = 100;
const KEY_TOLERANCE = KEY_STEP - 1;
const DRAG_TOLERANCE = 14;
// Enter puts a found computer against this one's right edge.
const PLACE_TOLERANCE = 150;
const ARROWS = {[Gdk.KEY_Left]: [-1, 0], [Gdk.KEY_Right]: [1, 0], [Gdk.KEY_Up]: [0, -1], [Gdk.KEY_Down]: [0, 1]};
const BOARD_HEIGHT = 240;
// Room on the board around the arrangement: at the sides, above it for the
// computers' names, and under it.
const ROOM = {sides: 28, above: 48, below: 24};
// How far an island's outline stands off its screens, and its name off the outline.
const HULL = 8;
const CHIP_GAP = 6;
// This computer's island, as the layout's own screens have no peer.
const LOCAL = 'local';
const THIS_COMPUTER = 'This Computer';
// src/app/api.rs PeerState: a few words for a name chip, and its dot's color.
const STATES = {
    paired: ['Not connected', null],
    connecting: ['Connecting…', 'warning'],
    connected: ['Connected', 'success'],
    controlling_this: ['Controlling this computer', 'accent'],
    controlled_from_here: ['Controlled from here', 'accent'],
    unreachable: ['Offline', null],
};
// A paired computer that cannot take input now shows faded.
const AWAY = new Set([undefined, 'paired', 'unreachable']);
// The shelf of computers found on the network, under the board.
const FOUND_SIZE = [132, 64];
const SHELF_TOP = 48;
const SHELF_GAP = 8;
// The size the service gives a placed computer's tile (PEER_TILE_SIZE in
// src/daemon.rs) until that computer sends its own.
const NEW_TILE = [1920, 1080];
const PLACEABLE = new Set(['ready', 'duplicate_name']);
const OS_NAMES = {linux: 'Linux', macos: 'macOS', windows: 'Windows'};
// A key's mark is six hex digits; each of the first four picks one of these
// by its low three bits, as the Mac app does, so a key looks the same on
// every computer.
const MARK_COLORS = ['#E5484D', '#F76B15', '#FFC53D', '#30A46C', '#12A594', '#0090FF', '#8E4EC6', '#D6409F'];
// This theme's buttons are restyled at application priority, so every state
// is set here. Color on the board means the selection or a crossing.
const CSS = `
.zf-shelf { border: 1.5px dashed alpha(currentColor, 0.25); border-radius: 12px; padding: 8px 12px; }
.zf-mark { min-width: 7px; min-height: 7px; border-radius: 2px; }
${MARK_COLORS.map((color, index) => `.zf-mark-${index} { background: ${color}; }`).join('\n')}
button.zf-screen { min-width: 0; min-height: 0; padding: 0; border-radius: 6px; background-color: @view_bg_color; background-image: none;
    box-shadow: inset 0 0 0 1px alpha(currentColor, 0.2), 0 1px 2px alpha(black, 0.06); }
button.zf-screen:hover { box-shadow: inset 0 0 0 1px alpha(currentColor, 0.38), 0 1px 2px alpha(black, 0.06); }
button.zf-screen.zf-away { opacity: 0.62; filter: grayscale(0.9); }
button.zf-screen.zf-moving { box-shadow: inset 0 0 0 1px alpha(currentColor, 0.38), 0 6px 14px alpha(black, 0.25); }
button.zf-screen.zf-refused { box-shadow: inset 0 0 0 2px @error_color, 0 6px 14px alpha(black, 0.25); }
.zf-screen-name { font-size: 0.75em; font-weight: 600; padding: 1px 5px; margin: 5px; border-radius: 4px; background: alpha(@view_bg_color, 0.75); }
button.zf-chip { min-height: 24px; padding: 0 9px; border-radius: 999px; background-color: transparent; background-image: none; box-shadow: none; }
button.zf-chip:hover { background-image: image(alpha(currentColor, 0.08)); }
button.zf-chip:active { background-image: image(alpha(currentColor, 0.12)); }
button.zf-chip.zf-selected { background-color: @window_bg_color; box-shadow: 0 0 0 2px @accent_bg_color, 0 1px 3px alpha(black, 0.12); }
button.zf-done { min-height: 26px; padding: 0 12px; border-radius: 999px; }
.zf-dot { min-width: 8px; min-height: 8px; border-radius: 999px; box-shadow: inset 0 0 0 1.5px alpha(currentColor, 0.45); }
.zf-dot-success { background: @success_color; box-shadow: none; }
.zf-dot-warning { background: @warning_color; box-shadow: none; }
.zf-dot-accent { background: @accent_bg_color; box-shadow: none; }
`;
// [red, green, blue] from 0 to 1, for cairo.
const channels = hex => [1, 3, 5].map(at => Number.parseInt(hex.slice(at, at + 2), 16) / 255);

function button(label, action, css = []) {
    const widget = new Gtk.Button({label, valign: Gtk.Align.CENTER, css_classes: css});
    widget.connect('clicked', action);
    return widget;
}

let styled = false;
function addStyle() {
    if (styled) return;
    styled = true;
    const provider = new Gtk.CssProvider();
    provider.load_from_string(CSS);
    Gtk.StyleContext.add_provider_for_display(Gdk.Display.get_default(), provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION);
}

// The palette indexes of a mark's four squares, from its six hex digits.
export function markColors(seed) {
    if (!/^[0-9a-f]{6}$/i.test(seed ?? '')) return [];
    return [...seed.slice(0, 4)].map(digit => Number.parseInt(digit, 16) & 7);
}

// Draws a mark into `box` as four small squares, or nothing without one.
function showMark(box, seed) {
    if (box._seed === seed) return;
    box._seed = seed;
    for (let child = box.get_first_child(); child; child = box.get_first_child()) box.remove(child);
    for (const color of markColors(seed)) box.append(new Gtk.Box({css_classes: ['zf-mark', `zf-mark-${color}`], valign: Gtk.Align.CENTER}));
    box.visible = !!box.get_first_child();
    // The text form a notification or `zflow nearby` gives.
    box.tooltip_text = box.visible ? `Mark ${seed}` : null;
}

function toggleClass(widget, name, on) {
    if (on) widget.add_css_class(name);
    else widget.remove_css_class(name);
}

// Whose screen a layout monitor is: this computer's, or a paired one's.
const ownerOf = monitor => monitor.peer ? `peer:${monitor.peer}` : LOCAL;

function dotClasses(state) {
    const color = STATES[state]?.[1];
    return ['zf-dot', ...color ? [`zf-dot-${color}`] : []];
}

// The accent libadwaita 1.6 reports, else GNOME's blue.
function accent() {
    const color = Adw.StyleManager.get_default().get_accent_color_rgba?.();
    return color ? [color.red, color.green, color.blue] : channels('#3584E4');
}

function roundedRect(cr, x, y, width, height, radius) {
    const r = Math.min(radius, width / 2, height / 2);
    cr.newSubPath();
    cr.arc(x + width - r, y + r, r, -Math.PI / 2, 0);
    cr.arc(x + width - r, y + height - r, r, 0, Math.PI / 2);
    cr.arc(x + r, y + height - r, r, Math.PI / 2, Math.PI);
    cr.arc(x + r, y + r, r, Math.PI, 1.5 * Math.PI);
    cr.closePath();
}

// A seam's badge: arrows each way across it on an accent dot, ⇄ across an
// upright seam and ⇅ across one lying flat.
function drawBadge(cr, x, y, vertical, color, strength) {
    cr.save();
    cr.setSourceRGBA(...color, strength);
    cr.arc(x, y, 9, 0, 2 * Math.PI);
    cr.fill();
    cr.translate(x, y);
    if (!vertical) cr.rotate(Math.PI / 2);
    cr.setSourceRGBA(1, 1, 1, 1);
    cr.setLineWidth(1.4);
    cr.setLineCap(Cairo.LineCap.ROUND);
    cr.setLineJoin(Cairo.LineJoin.ROUND);
    cr.moveTo(-4.5, -2);
    cr.lineTo(4.5, -2);
    cr.moveTo(2.5, -4);
    cr.lineTo(4.5, -2);
    cr.lineTo(2.5, 0);
    cr.moveTo(4.5, 2);
    cr.lineTo(-4.5, 2);
    cr.moveTo(-2.5, 0);
    cr.lineTo(-4.5, 2);
    cr.lineTo(-2.5, 4);
    cr.stroke();
    cr.restore();
}

// The badge a crossing's row starts with, as its seam shows it.
function badge(vertical) {
    const area = new Gtk.DrawingArea({content_width: 20, content_height: 20, valign: Gtk.Align.CENTER});
    area.set_draw_func((_area, cr) => {
        drawBadge(cr, 10, 10, vertical, accent(), 1);
        cr.$dispose();
    });
    return area;
}

// "27″ · 2560 × 1440", with what the system knows of the two.
function screenDetail(display) {
    if (!display) return '';
    const parts = [];
    const {width_mm: width, height_mm: height, bounds} = display;
    if (width > 0 && height > 0) parts.push(`${Number((Math.hypot(width, height) / 25.4).toFixed(1))}″`);
    if (bounds) parts.push(`${bounds.width} × ${bounds.height}`);
    return parts.join(' · ');
}

// "2 screens · Connected"
function summary({peer, tiles}) {
    const screens = tiles.length ? `${tiles.length} ${tiles.length === 1 ? 'screen' : 'screens'}` : 'Not on the arrangement';
    return [screens, peer && STATES[peer.state]?.[0]].filter(Boolean).join(' · ');
}

export class Settings {
    constructor(window) {
        this.window = window;
        this._updating = false;
        this._busy = false;
        this._disposed = false;
        this._healthKey = '';
        this._shortcutKey = '';
        this._peers = [];
        this._keyboards = new Map();
        this._controls = new Map();
        this._scrolls = new Map();
        this._lensRows = new Map();
        // The rows each inspector group shows.
        this._filled = new Map();
        this._inspectorKey = null;
        this._inspectorRows = [];
        this._adjustRow = null;
        this.page = new Adw.PreferencesPage({title: 'zflow', icon_name: 'io.zflow.zflow-symbolic'});
        const sharing = new Adw.PreferencesGroup();
        this._status = new Adw.ActionRow({title: 'Starting zflow…', subtitle: 'Share your keyboard, pointer, and trackpad.', subtitle_lines: 3, use_markup: false});
        this._statusIcon = new Gtk.Image({icon_name: 'input-mouse-symbolic'});
        this._status.add_prefix(this._statusIcon);
        sharing.add(this._status);
        this._sharing = new Adw.SwitchRow({title: 'Input Sharing', subtitle: 'Allow input to move between paired computers.', sensitive: false});
        this._sharing.connect('notify::active', () => {
            if (!this._updating) this._run({command: 'set_sharing', enabled: this._sharing.active});
        });
        sharing.add(this._sharing);
        this.page.add(sharing);
        this._health = new Adw.PreferencesGroup({title: 'Checks', visible: false});
        this.page.add(this._health);
        // Requests that failed; checks that fail are in the group above.
        this._errorGroup = new Adw.PreferencesGroup({visible: false});
        this._error = new Adw.ActionRow({title: 'Needs attention', subtitle_lines: 0});
        this._error.add_prefix(new Gtk.Image({icon_name: 'dialog-warning-symbolic'}));
        this._errorGroup.add(this._error);
        this.page.add(this._errorGroup);

        addStyle();
        this._computers = new Adw.PreferencesGroup({title: 'Computers', description: NO_LAYOUT});
        this._addButton = button('Add by Address…', () => this._openAddAddress(), ['flat']);
        this._computers.header_suffix = this._addButton;
        // A fresh install's pairing window, and the computer it took.
        this._windowRow = new Adw.ActionRow({title: '', subtitle_lines: 0, use_markup: false, visible: false});
        this._windowMark = new Gtk.Box({spacing: 2});
        this._windowRow.add_prefix(this._windowMark);
        this._computers.add(this._windowRow);
        this._dismissed = 0;
        this._joinedRow = new Adw.ActionRow({title: '', subtitle: 'It can share this computer’s keyboard and mouse. Not yours?', subtitle_lines: 0, use_markup: false, visible: false});
        this._joinedMark = new Gtk.Box({spacing: 2});
        this._joinedRow.add_prefix(this._joinedMark);
        this._joinedRow.add_suffix(button('Forget', () => this._forgetJoined(), ['destructive-action']));
        const dismiss = new Gtk.Button({icon_name: 'window-close-symbolic', tooltip_text: 'Dismiss', valign: Gtk.Align.CENTER, css_classes: ['flat']});
        dismiss.connect('clicked', () => this._dismissJoined());
        this._joinedRow.add_suffix(dismiss);
        this._computers.add(this._joinedRow);
        // The arrangement: each computer's screens as an island with its name
        // above it, lit seams where the pointer crosses to another computer,
        // and computers found on the network on a shelf under it until one is
        // dragged in. Screens and names are buttons, so they take keyboard
        // focus and have names.
        this._tiles = new Map();
        this._islands = new Map();
        this._found = new Map();
        this._layoutKey = '';
        this._foundKey = '';
        this._seams = [];
        this._seamKey = '';
        // Seams fade while sharing is off, since the pointer crosses none.
        this._crossing = true;
        this._selection = null;
        // The computer under the pointer, which shows its outline and screen names.
        this._hovered = null;
        // The computer whose screens move one by one.
        this._adjusting = null;
        this._pressed = null;
        this._lastPress = null;
        this._drag = null;
        this._asking = false;
        // Where a found computer being carried has its middle.
        this._carried = null;
        this._boardSize = [440, BOARD_HEIGHT];
        this._shelfHeight = 0;
        this._board = new Gtk.Fixed();
        this._shelf = new Gtk.Box({orientation: Gtk.Orientation.VERTICAL, spacing: 2, css_classes: ['zf-shelf'], visible: false});
        this._shelf.append(new Gtk.Label({label: 'Found on your network', xalign: 0, css_classes: ['heading']}));
        this._shelf.append(new Gtk.Label({label: 'Drag one next to a screen to add it.', xalign: 0, css_classes: ['caption', 'dim-label']}));
        this._board.put(this._shelf, 0, 0);
        // One drag for the whole box. On a tile, it would measure from the
        // tile that moves under it. It sees a press before the tile's button
        // does, as a scrolled window's drag does, and takes it only once the
        // pointer moves.
        const drag = new Gtk.GestureDrag({propagation_phase: Gtk.PropagationPhase.CAPTURE});
        drag.connect('drag-begin', (_drag, x, y) => this._press(x, y));
        drag.connect('drag-update', (_drag, dx, dy) => this._dragTile(drag, dx, dy));
        drag.connect('drag-end', (_drag, dx, dy) => this._dropTile(dx, dy));
        this._board.add_controller(drag);
        const motion = new Gtk.EventControllerMotion();
        motion.connect('motion', (_motion, x, y) => this._hover(x, y));
        motion.connect('leave', () => this._hover(null, null));
        this._board.add_controller(motion);
        // A Gtk.Fixed does not report its width, so the area under it does.
        // It draws the islands' outlines; the one over the board draws the
        // seams, which lie along the screens' edges.
        this._area = new Gtk.DrawingArea({content_height: BOARD_HEIGHT, hexpand: true});
        this._area.set_draw_func((area, cr, width) => this._drawIslands(area, cr, width));
        this._area.connect('resize', (_area, width) => {
            this._boardSize = [width, BOARD_HEIGHT];
            this._placeTiles();
            this._placeShelf();
        });
        this._seamArea = new Gtk.DrawingArea({can_target: false});
        this._seamArea.set_draw_func((area, cr) => this._drawSeams(area, cr));
        const canvas = new Gtk.Overlay({child: this._area, overflow: Gtk.Overflow.HIDDEN});
        canvas.add_overlay(this._board);
        canvas.add_overlay(this._seamArea);
        this._layout = new Adw.PreferencesRow({title: 'Layout', child: canvas, activatable: false, focusable: false, visible: false});
        this._computers.add(this._layout);
        this.page.add(this._computers);
        // Under the arrangement, as GNOME Settings shows a display's settings
        // under its arrangement: every computer, or the selected one's
        // crossings, screens and settings.
        this._inspector = new Adw.PreferencesGroup();
        this._crossings = new Adw.PreferencesGroup({title: 'Crossings', visible: false});
        this._screens = new Adw.PreferencesGroup({title: 'Screens', visible: false});
        this._peerSettings = new Adw.PreferencesGroup({title: 'Settings', visible: false});
        for (const group of [this._inspector, this._crossings, this._screens, this._peerSettings]) this.page.add(group);
        const switching = new Adw.PreferencesGroup();
        this._pause = new Adw.SwitchRow({title: 'Pause at Edges', subtitle: 'Rest the pointer against an edge for a moment before it crosses.', sensitive: false});
        this._pause.connect('notify::active', () => {
            if (!this._updating) this._run({command: 'set_switching', pause_at_edges: this._pause.active});
        });
        switching.add(this._pause);
        this.page.add(switching);
        this._shortcuts = new Adw.PreferencesGroup({title: 'Shortcuts', visible: false});
        this.page.add(this._shortcuts);
        const clipboard = new Adw.PreferencesGroup();
        this._clipboard = new Adw.SwitchRow({title: 'Share Clipboard', subtitle: 'Text and images go with the pointer to the other computer. Never files.', sensitive: false});
        this._clipboard.connect('notify::active', () => {
            if (!this._updating) this._run({command: 'set_clipboard', share: this._clipboard.active});
        });
        clipboard.add(this._clipboard);
        this.page.add(clipboard);
        const preferences = new Adw.PreferencesGroup();
        this._login = new Adw.SwitchRow({title: 'Start at Login', subtitle: 'Keep zflow available after you close settings.'});
        this._login.connect('notify::active', () => {
            if (!this._updating) this._run({command: 'set_autostart', enabled: this._login.active});
        });
        preferences.add(this._login);
        this.page.add(preferences);
        // Rows only this computer has, under the shared ones.
        const local = new Adw.PreferencesGroup({title: 'This Computer'});
        const logs = new Adw.ActionRow({title: 'Service Log', subtitle: 'Follow what zflow’s background service is doing.'});
        this._logs = button('Open', () => this._run({command: 'open_logs'}));
        logs.add_suffix(this._logs);
        local.add(logs);
        this.page.add(local);
        this._help = new Adw.PreferencesGroup();
        this.page.add(this._help);
        this.client = new Client(snapshot => this._update(snapshot), true);
        this.client.start();
    }

    _showError(message) {
        if (this._disposed) return;
        this._actionError = message;
        this._error.subtitle = message ?? '';
        this._errorGroup.visible = !!message;
    }

    async _run(request) {
        if (this._busy) return false;
        this._busy = true;
        this._sharing.sensitive = this._login.sensitive = this._addButton.sensitive = this._pause.sensitive = this._clipboard.sensitive = false;
        for (const row of [...this._keyboards.values(), ...this._controls.values(), ...this._scrolls.values(), ...this._lensRows.values()]) row.sensitive = false;
        this._showError(null);
        let success = false;
        try { await this.client.call(request); success = true; }
        catch (error) { this._showError(error.message); }
        finally {
            this._busy = false;
            if (!this._disposed) await this.client.refresh();
        }
        return success;
    }

    _update(snapshot) {
        if (this._disposed) return;
        // An agent from another API level sends a snapshot this window cannot read.
        const known = compatible(snapshot);
        const online = known && snapshot.sharing !== null;
        const state = known ? snapshot.status.state : 'attention';
        this._updating = true;
        this._status.title = statusText(snapshot);
        this._statusIcon.icon_name = state === 'attention' ? 'dialog-warning-symbolic'
            : state === 'paused' ? 'media-playback-pause-symbolic' : 'input-mouse-symbolic';
        this._sharing.active = online && snapshot.sharing;
        this._sharing.sensitive = online && !this._busy;
        this._pause.active = (known && snapshot.pause_at_edges) ?? false;
        this._pause.sensitive = online && snapshot.pause_at_edges !== null && !this._busy;
        // An agent from before clipboard sharing leaves it out.
        const share = known ? (snapshot.share_clipboard ?? null) : null;
        this._clipboard.active = share ?? false;
        this._clipboard.sensitive = online && share !== null && !this._busy;
        this._login.active = (known && snapshot.autostart) ?? false;
        this._login.sensitive = known && snapshot.autostart !== null && !this._busy;
        this._addButton.sensitive = online && !this._busy;
        this._updating = false;
        this._error.subtitle = this._actionError || (known ? '' : snapshot?.error ?? 'Update zflow so this window and the service match.');
        this._errorGroup.visible = !!this._error.subtitle;
        this._help.description = `Changes save automatically. Advanced settings are in ${known ? snapshot.config_path : '/etc/zflow/zflow.toml'}.`;
        this._updateHealth(known ? snapshot.health : []);
        const peers = this._peers = known ? snapshot.peers : [];
        // An agent from before the shelf leaves these out.
        this._updateLayout(known ? snapshot.layout : null, known ? snapshot.own_mark ?? null : null, peers);
        this._updateSeams(known ? snapshot.seams ?? [] : [], online && snapshot.sharing);
        this._updateFound(known ? snapshot.unplaced ?? [] : []);
        this._updateWindow(known ? snapshot.pairing_window ?? null : null);
        this._updateJoined(known ? snapshot.notices ?? [] : [], peers);
        this._updateInspector();
        this._updateShortcuts(known ? snapshot.shortcuts : []);
    }

    _updateWindow(window) {
        const open = window?.state === 'open';
        this._windowRow.visible = open;
        if (!open) return;
        const holding = window.holding;
        if (holding) {
            this._windowRow.title = `Adding ${holding.name}`;
            this._windowRow.subtitle = `In ${Math.ceil(holding.ms_left / 1000)} s, unless another computer shows up.`;
        } else {
            const minutes = Math.max(1, Math.ceil((window.seconds_left ?? 0) / 60));
            this._windowRow.title = 'This computer is new';
            this._windowRow.subtitle = `For the next ${minutes} ${minutes === 1 ? 'minute' : 'minutes'}, a new computer that shows up alone joins by itself. Or drag one into place below.`;
        }
        showMark(this._windowMark, holding?.mark ?? null);
    }

    // The newest computer that joined, while it is still here, until dismissed.
    _updateJoined(notices, peers) {
        const names = new Set(peers.map(({name}) => name));
        const joined = notices.findLast(notice => notice.kind === 'joined' && notice.id > this._dismissed && names.has(notice.name));
        this._joined = joined ?? null;
        this._joinedRow.visible = !!joined;
        if (joined) this._joinedRow.title = `${joined.name} joined`;
        showMark(this._joinedMark, joined?.mark ?? null);
    }

    _dismissJoined() {
        if (this._joined) this._dismissed = this._joined.id;
        this._joinedRow.visible = false;
    }

    async _forgetJoined() {
        const joined = this._joined;
        if (!joined) return;
        this._dismissJoined();
        await this._run({command: 'forget', name: joined.name});
    }

    _updateHealth(rows) {
        const key = JSON.stringify(rows);
        if (key === this._healthKey) return;
        this._healthKey = key;
        for (const row of this._healthRows ?? []) this._health.remove(row);
        this._healthRows = rows.map(({level, title, detail, action}) => {
            const row = new Adw.ActionRow({title, subtitle: detail, subtitle_lines: 0, use_markup: false});
            row.add_prefix(new Gtk.Image({icon_name: HEALTH_ICONS[level] ?? HEALTH_ICONS.warning}));
            if (action) row.add_suffix(button(action.label, () => this._run({command: action.command})));
            this._health.add(row);
            return row;
        });
        this._health.visible = rows.length > 0;
    }

    _updateLayout(layout, ownMark, peers) {
        const monitors = (layout?.monitors ?? []).filter(m => m.display?.active !== false);
        this._layout.visible = monitors.length > 0;
        this._computers.description = monitors.length ? LAYOUT_HINT : NO_LAYOUT;
        const known = new Map(peers.map(peer => [peer.name, peer]));
        // Keep tiles, and the focused one, while unchanged snapshots arrive. A
        // move keeps its tiles too, so arrow keys can move them again.
        const key = JSON.stringify([monitors, ownMark, peers.map(({name, state, mark}) => [name, state, mark])]);
        if (key === this._layoutKey) return;
        this._layoutKey = key;
        const ids = new Set(monitors.map(({id}) => id));
        for (const [id, tile] of this._tiles) {
            if (ids.has(id)) continue;
            this._board.remove(tile.button);
            this._tiles.delete(id);
        }
        // Each computer's screens, in the order the layout first names them.
        const owners = new Map();
        for (const monitor of monitors) {
            const tile = this._tiles.get(monitor.id) ?? this._newTile(monitor);
            tile.monitor = monitor;
            tile.owner = ownerOf(monitor);
            if (!owners.has(tile.owner)) owners.set(tile.owner, []);
            owners.get(tile.owner).push(tile);
        }
        for (const [owner, island] of this._islands) {
            if (owners.has(owner)) continue;
            this._board.remove(island.chip);
            this._board.remove(island.done);
            this._islands.delete(owner);
        }
        for (const [owner, tiles] of owners) {
            const name = tiles[0].monitor.peer;
            const peer = name ? known.get(name) ?? null : null;
            this._showIsland(this._islands.get(owner) ?? this._newIsland(owner), tiles, peer, name ? peer?.mark ?? null : ownMark);
        }
        this._raiseChips();
        // A drop ends once a snapshot has its screen somewhere else.
        const drag = this._drag;
        const anchor = drag && this._tiles.get(drag.anchor.id)?.monitor;
        if (drag?.dropped && (anchor?.x !== drag.anchor.x || anchor?.y !== drag.anchor.y)) this._drag = null;
        this._placeTiles();
    }

    _updateSeams(seams, crossing) {
        const key = JSON.stringify([seams, crossing]);
        if (key === this._seamKey) return;
        this._seamKey = key;
        this._seams = seams;
        this._crossing = crossing;
        this._seamArea.queue_draw();
    }

    _newTile(monitor) {
        const label = new Gtk.Label({ellipsize: Pango.EllipsizeMode.END, xalign: 0, halign: Gtk.Align.START, valign: Gtk.Align.END, hexpand: true, vexpand: true, css_classes: ['zf-screen-name']});
        // A computer that has not said what screens it has shows an icon.
        const icon = new Gtk.Image({icon_name: 'computer-symbolic', hexpand: true, vexpand: true, css_classes: ['dim-label']});
        const content = new Gtk.Box();
        content.append(icon);
        content.append(label);
        const button = new Gtk.Button({child: content, css_classes: ['zf-screen']});
        // Where the screen sits on the board, in pixels, and where it shows
        // while a drag carries it.
        const tile = {button, label, icon, monitor, owner: ownerOf(monitor), x: 0, y: 0, size: [0, 0], shown: [0, 0, 0, 0]};
        button.connect('clicked', () => this._select(tile.owner));
        // Out of adjusting, a screen carries its whole computer.
        button.add_controller(this._keys(() => tile, () => this._adjusting !== tile.owner));
        this._board.put(button, 0, 0);
        this._tiles.set(monitor.id, tile);
        return tile;
    }

    // A computer's name above its screens. Dragging it moves the computer;
    // double-clicking it, or one of its screens, adjusts them one by one.
    _newIsland(owner) {
        const name = new Gtk.Label({ellipsize: Pango.EllipsizeMode.END, max_width_chars: 16, css_classes: ['caption-heading']});
        // Its state is a dot here, in words in its tooltip and the inspector,
        // and its mark is in the inspector, so names stay small on the board.
        const dot = new Gtk.Box({css_classes: ['zf-dot'], valign: Gtk.Align.CENTER});
        const content = new Gtk.Box({spacing: 6});
        for (const child of [name, dot]) content.append(child);
        const chip = new Gtk.Button({child: content, css_classes: ['zf-chip']});
        const done = new Gtk.Button({label: 'Done', tooltip_text: 'Move its screens together again', css_classes: ['zf-done', 'suggested-action'], visible: false});
        // Where its name sits on the board, and on which side of its outline.
        const island = {owner, chip, done, name, dot, seed: null, tiles: [], x: 0, y: 0, size: [0, 0], spot: null, doneAt: {x: 0, y: 0, size: [0, 0]}};
        chip.connect('clicked', () => this._select(owner));
        done.connect('clicked', () => this._adjust(owner, false));
        chip.add_controller(this._keys(() => island.tiles[0], () => true));
        chip.update_property([Gtk.AccessibleProperty.DESCRIPTION], ['Drag it or use the arrow keys to move this computer. Double-click it to move its screens one by one.']);
        this._board.put(chip, 0, 0);
        this._board.put(done, 0, 0);
        this._islands.set(owner, island);
        return island;
    }

    _showIsland(island, tiles, peer, seed) {
        const name = tiles[0].monitor.peer;
        island.tiles = tiles;
        island.seed = seed;
        island.name.label = name ?? THIS_COMPUTER;
        const state = peer?.state;
        island.dot.visible = !!name;
        island.dot.css_classes = dotClasses(state);
        island.chip.tooltip_text = name ? [name, STATES[state]?.[0]].filter(Boolean).join(' · ') : THIS_COMPUTER;
        const away = !!name && AWAY.has(state);
        for (const tile of tiles) {
            const {monitor} = tile;
            tile.button.css_classes = ['zf-screen', ...away ? ['zf-away'] : []];
            tile.label.label = monitor.display?.name ?? monitor.label;
            tile.button.tooltip_text = monitor.label;
            tile.button.update_property([Gtk.AccessibleProperty.DESCRIPTION], [`${island.name.label}. Position ${monitor.x}, ${monitor.y}. Drag it or use the arrow keys to move it.`]);
        }
    }

    // Names draw over the screens, and Done beside its name.
    _raiseChips() {
        for (const island of this._islands.values()) {
            island.chip.insert_before(this._board, null);
            island.done.insert_before(this._board, null);
        }
    }

    // Arrow keys move a computer, or one of its screens while it is being
    // adjusted. Escape lets go of the selected computer.
    _keys(tile, computer) {
        const keys = new Gtk.EventControllerKey();
        keys.connect('key-pressed', (_keys, keyval) => {
            if (keyval === Gdk.KEY_Escape) {
                if (this._selection === null) return false;
                this._select(null);
                return true;
            }
            const step = ARROWS[keyval];
            const anchor = tile();
            if (!step || !anchor) return false;
            const {id, x, y} = anchor.monitor;
            this._moveTile(id, x + step[0] * KEY_STEP, y + step[1] * KEY_STEP, KEY_TOLERANCE, computer());
            return true;
        });
        return keys;
    }

    _select(owner, focus = false) {
        if (owner !== this._selection) {
            this._selection = owner;
            if (this._adjusting !== owner) this._adjusting = null;
            this._showSelection();
        }
        if (focus) this._islands.get(owner)?.chip.grab_focus();
    }

    // Whether a computer's screens move one by one instead of together.
    _adjust(owner, on) {
        this._selection = owner;
        this._adjusting = on ? owner : null;
        this._showSelection();
    }

    _showSelection() {
        for (const island of this._islands.values()) toggleClass(island.chip, 'zf-selected', island.owner === this._selection);
        this._placeTiles();
        this._updateInspector();
    }

    // The computer whose outline or name is under the pointer, or none.
    _hover(x, y) {
        const inside = ([left, top, width, height]) => x >= left && x < left + width && y >= top && y < top + height;
        const island = x === null ? null : [...this._islands.values()].find(island => {
            const hull = this._hull(island);
            return hull && (inside(hull) || inside([island.x, island.y, ...island.size]));
        });
        const owner = island?.owner ?? null;
        if (owner === this._hovered) return;
        this._hovered = owner;
        this._placeTiles();
    }

    // The scale and origin that center the layout on the board, with room
    // above it for the computers' names, as the Mac app fits it.
    _fit(monitors, width, height) {
        const left = Math.min(...monitors.map(m => m.x));
        const top = Math.min(...monitors.map(m => m.y));
        const right = Math.max(...monitors.map(m => m.x + m.width));
        const bottom = Math.max(...monitors.map(m => m.y + m.height));
        const scale = Math.max(0.001, Math.min(
            (width - 2 * ROOM.sides) / Math.max(right - left, 1),
            (height - ROOM.above - ROOM.below) / Math.max(bottom - top, 1),
            0.12));
        return {
            scale, left, top,
            offsetX: (width - (right - left) * scale) / 2,
            offsetY: ROOM.above + (height - ROOM.above - ROOM.below - (bottom - top) * scale) / 2,
        };
    }

    // A monitor on the board as [x, y, width, height]. Every screen rounds
    // its edges alike, so touching edges stay touching.
    _rect(monitor, fit, shift) {
        const x = value => Math.round(fit.offsetX + (value - fit.left) * fit.scale + shift[0]);
        const y = value => Math.round(fit.offsetY + (value - fit.top) * fit.scale + shift[1]);
        const [left, top] = [x(monitor.x), y(monitor.y)];
        return [left, top, Math.max(1, x(monitor.x + monitor.width) - left), Math.max(1, y(monitor.y + monitor.height) - top)];
    }

    // Scales the layout to fit the board and centers it. A drag keeps the
    // scale it began with, so nothing rescales under the pointer.
    _placeTiles() {
        const tiles = [...this._tiles.values()];
        if (tiles.length) {
            const [width, height] = this._boardSize;
            const fit = this._fitted = this._drag?.fit ?? this._fit(tiles.map(tile => tile.monitor), width, height);
            this._scale = fit.scale;
            // Where layout unit (left, top) is on the board, for computers
            // dropped from the shelf.
            this._origin = {left: fit.left, top: fit.top, offsetX: fit.offsetX, offsetY: fit.offsetY};
            const drag = this._drag;
            const shift = drag ? this._shift(drag) : [0, 0];
            const refused = !!drag && drag.asked !== null && drag.landing === null;
            for (const tile of tiles) {
                const home = this._rect(tile.monitor, fit, [0, 0]);
                [tile.x, tile.y] = home;
                tile.size = home.slice(2);
                const moving = !!drag && this._moves(drag, tile);
                const shown = tile.shown = moving ? this._rect(tile.monitor, fit, shift) : home;
                tile.button.set_size_request(shown[2], shown[3]);
                this._board.move(tile.button, shown[0], shown[1]);
                toggleClass(tile.button, 'zf-moving', moving);
                toggleClass(tile.button, 'zf-refused', moving && refused);
                // Only the selected or hovered computer names its screens,
                // and a screen too small for its name shows none.
                const named = tile.owner === this._selection || tile.owner === this._hovered;
                tile.label.visible = named && !!tile.monitor.display && shown[2] >= 64 && shown[3] >= 30;
                tile.icon.visible = !tile.monitor.display && Math.min(shown[2], shown[3]) >= 24;
            }
            this._placeChips();
        }
        this._area.queue_draw();
        this._seamArea.queue_draw();
    }

    // An island's screens as shown, with room around them.
    _hull(island) {
        if (!island.tiles.length) return null;
        const rects = island.tiles.map(tile => tile.shown);
        const left = Math.min(...rects.map(r => r[0]));
        const top = Math.min(...rects.map(r => r[1]));
        const right = Math.max(...rects.map(r => r[0] + r[2]));
        const bottom = Math.max(...rects.map(r => r[1] + r[3]));
        return [left - HULL, top - HULL, right - left + 2 * HULL, bottom - top + 2 * HULL];
    }

    // A computer's name sits over its outline's top left corner, or at
    // another corner or just inside, wherever it covers the least of the
    // other computers and names. A drag keeps each name at its spot, so none
    // jumps under the pointer.
    _placeChips() {
        const [width, height] = this._boardSize;
        const islands = [...this._islands.values()];
        const hulls = new Map(islands.map(island => [island, this._hull(island)]));
        const covered = (a, b) => Math.max(0, Math.min(a[0] + a[2], b[0] + b[2]) - Math.max(a[0], b[0]))
            * Math.max(0, Math.min(a[1] + a[3], b[1] + b[3]) - Math.max(a[1], b[1]));
        const taken = [];
        for (const island of islands) {
            const hull = hulls.get(island);
            island.chip.visible = !!hull;
            island.done.visible = !!hull && this._adjusting === island.owner;
            if (!hull) continue;
            // A Gtk.Fixed gives a child its minimum size, which an ellipsized
            // name would shrink to nothing.
            island.chip.set_size_request(-1, -1);
            const [, natural] = island.chip.measure(Gtk.Orientation.HORIZONTAL, -1);
            const [, chipHeight] = island.chip.measure(Gtk.Orientation.VERTICAL, -1);
            const chipWidth = Math.min(natural, width - 8);
            island.chip.set_size_request(chipWidth, -1);
            const [, doneWidth] = island.done.visible ? island.done.measure(Gtk.Orientation.HORIZONTAL, -1) : [0, 0];
            const span = chipWidth + (doneWidth ? CHIP_GAP + doneWidth : 0);
            const fit = x => Math.round(Math.max(4, Math.min(x, width - span - 4)));
            const [start, end] = [fit(hull[0]), fit(hull[0] + hull[2] - span)];
            const [above, below, inside] = [hull[1] - chipHeight - CHIP_GAP, hull[1] + hull[3] + CHIP_GAP, hull[1] + HULL + 4];
            const spots = [[start, above], [end, above], [start, below], [end, below], [start, inside]];
            const others = [...islands.filter(other => other !== island && hulls.get(other)).map(other => hulls.get(other)), ...taken];
            // Off the board counts as covering everything.
            const cost = ([x, y]) => y < 4 || y + chipHeight > height - 4 ? Infinity
                : others.reduce((sum, other) => sum + covered([x, y, span, chipHeight], other), 0);
            if (!this._drag || island.spot === null) {
                const costs = spots.map(cost);
                island.spot = costs.indexOf(Math.min(...costs));
            }
            const [x, wanted] = spots[island.spot];
            const y = Math.round(Math.max(4, Math.min(wanted, height - chipHeight - 4)));
            taken.push([x, y, span, chipHeight]);
            Object.assign(island, {x, y, size: [chipWidth, chipHeight]});
            this._board.move(island.chip, x, y);
            if (!island.done.visible) continue;
            const [, doneHeight] = island.done.measure(Gtk.Orientation.VERTICAL, -1);
            island.doneAt = {x: x + chipWidth + CHIP_GAP, y: y + Math.round((chipHeight - doneHeight) / 2), size: [doneWidth, doneHeight]};
            this._board.move(island.done, island.doneAt.x, island.doneAt.y);
        }
    }

    // The selected and hovered computers' outlines, on a faint dotted
    // backdrop like a desk mat.
    _drawIslands(area, cr, width) {
        const fg = area.get_color();
        cr.setSourceRGBA(fg.red, fg.green, fg.blue, 0.07);
        for (let x = 9; x < width; x += 18) {
            for (let y = 9; y < this._boardSize[1]; y += 18) {
                cr.newSubPath();
                cr.arc(x, y, 0.8, 0, 2 * Math.PI);
            }
        }
        cr.fill();
        if (this._layout.visible) {
            const color = accent();
            for (const island of this._islands.values()) {
                const selected = this._selection === island.owner;
                const hull = (selected || this._hovered === island.owner) && this._hull(island);
                if (!hull) continue;
                roundedRect(cr, ...hull, 14);
                cr.setSourceRGBA(...selected ? color : [fg.red, fg.green, fg.blue], selected ? 1 : 0.35);
                cr.setLineWidth(selected ? 2.5 : 1.5);
                cr.setDash(this._adjusting === island.owner ? [6, 5] : [], 0);
                cr.stroke();
            }
        }
        cr.$dispose();
    }

    // Lit seams where the pointer crosses to another computer, and dotted
    // joints between one computer's own screens. A found computer carried
    // over the board shows where it would land.
    _drawSeams(area, cr) {
        const fit = this._fitted;
        if (fit && this._layout.visible) {
            const fg = area.get_color();
            const color = accent();
            const strength = this._crossing ? 1 : 0.3;
            const seams = this._shownSeams();
            cr.setLineCap(Cairo.LineCap.ROUND);
            cr.setSourceRGBA(fg.red, fg.green, fg.blue, 0.5);
            cr.setLineWidth(2);
            cr.setDash([0.1, 5], 0);
            for (const seam of seams.filter(seam => seam.same_computer)) {
                const [x0, y0, x1, y1] = this._seamLine(seam, fit, 4);
                cr.moveTo(x0, y0);
                cr.lineTo(x1, y1);
                cr.stroke();
            }
            cr.setDash([], 0);
            for (const seam of seams.filter(seam => !seam.same_computer)) {
                const [x0, y0, x1, y1] = this._seamLine(seam, fit, 3);
                cr.moveTo(x0, y0);
                cr.lineTo(x1, y1);
                cr.setSourceRGBA(...color, 0.28 * strength);
                cr.setLineWidth(11);
                cr.strokePreserve();
                cr.setSourceRGBA(...color, strength);
                cr.setLineWidth(4);
                cr.stroke();
                // Arrows across it, where there is room.
                if ((seam.end - seam.start) * fit.scale > 34) drawBadge(cr, (x0 + x1) / 2, (y0 + y1) / 2, seam.vertical, color, strength);
            }
            const carried = this._carried;
            if (carried && carried[1] < this._boardSize[1]) {
                const [width, height] = NEW_TILE.map(size => size * fit.scale);
                roundedRect(cr, carried[0] - width / 2, carried[1] - height / 2, width, height, 6);
                cr.setSourceRGBA(...color, 1);
                cr.setLineWidth(1.5);
                cr.setDash([5, 4], 0);
                cr.stroke();
            }
        }
        cr.$dispose();
    }

    // While a drag waits for the engine, only the seams it cannot change show.
    _shownSeams() {
        const drag = this._drag;
        if (!drag) return this._seams;
        if (drag.landing) return drag.landing.seams;
        const moving = new Set([...this._tiles.values()].filter(tile => this._moves(drag, tile)).map(({monitor}) => monitor.id));
        return this._seams.filter(({a, b}) => !moving.has(a) && !moving.has(b));
    }

    // A seam on the board as [x0, y0, x1, y1], short of its ends by `inset`.
    _seamLine(seam, fit, inset) {
        const across = seam.vertical ? fit.offsetX + (seam.at - fit.left) * fit.scale : fit.offsetY + (seam.at - fit.top) * fit.scale;
        const along = seam.vertical ? fit.offsetY - fit.top * fit.scale : fit.offsetX - fit.left * fit.scale;
        const start = along + seam.start * fit.scale + inset;
        const end = Math.max(start, along + seam.end * fit.scale - inset);
        const line = Math.round(across);
        return seam.vertical ? [line, start, line, end] : [start, line, end, line];
    }

    // What a press on the board picked up. A second press on one computer
    // soon after the first adjusts its screens, or stops adjusting them.
    _press(x, y) {
        const hit = this._pressed = this._hit(x, y);
        this._pressAt = [x, y];
        const owner = hit?.island?.owner ?? null;
        const settings = Gtk.Settings.get_default();
        const now = GLib.get_monotonic_time() / 1000;
        const last = this._lastPress;
        const twice = owner !== null && last?.owner === owner
            && now - last.time <= (settings?.gtk_double_click_time ?? 400)
            && Math.hypot(x - last.x, y - last.y) <= (settings?.gtk_double_click_distance ?? 5);
        this._lastPress = twice || owner === null ? null : {owner, time: now, x, y};
        if (!twice) return;
        this._adjust(owner, this._adjusting !== owner);
        if (hit.tile) hit.computer = this._adjusting !== owner;
    }

    _hit(x, y) {
        const inside = ({x: left, y: top, size}) => x >= left && y >= top && x < left + size[0] && y < top + size[1];
        const islands = [...this._islands.values()];
        if (islands.some(island => island.done.visible && inside(island.doneAt))) return {done: true};
        const island = islands.find(island => island.chip.visible && inside(island));
        if (island) return {island, anchor: island.tiles[0], computer: true};
        // Later screens draw on top.
        const tile = [...this._tiles.values()].findLast(inside);
        if (tile) return {island: this._islands.get(tile.owner), anchor: tile, tile: true, computer: this._adjusting !== tile.owner};
        const found = [...this._found.values()].find(tile => inside(tile) && PLACEABLE.has(tile.found.state));
        return found ? {found} : null;
    }

    // Past a click, what was pressed follows the pointer: a found computer,
    // or a computer's screens, which the engine says where they would land.
    _dragTile(drag, dx, dy) {
        const pressed = this._pressed;
        if (!pressed || pressed.done) return;
        if (!pressed.dragging) {
            if (Math.hypot(dx, dy) < 3) return;
            pressed.dragging = true;
            // The button under the pointer lets go of the press, and a press
            // that dragged is not half a double-click.
            drag.set_state(Gtk.EventSequenceState.CLAIMED);
            this._lastPress = null;
            if (pressed.found) {
                pressed.found.dragging = true;
                pressed.found.button.insert_before(this._board, null);
            } else {
                this._startMove(pressed);
            }
        }
        if (pressed.found) {
            const tile = pressed.found;
            const [x, y] = this._dragged(tile, dx, dy);
            this._board.move(tile.button, x, y);
            this._carried = [x + tile.size[0] / 2, y + tile.size[1] / 2];
            this._seamArea.queue_draw();
            return;
        }
        this._drag.pixels = this._travel(this._drag, dx, dy);
        this._placeTiles();
        this._ask();
    }

    // Sends where the screens were dropped, in layout units.
    _dropTile(dx, dy) {
        const pressed = this._pressed;
        this._pressed = null;
        if (!pressed?.dragging) {
            // A click beside the computers lets go of the selected one.
            if (!pressed && this._pressAt?.[1] < this._boardSize[1]) this._select(null);
            return;
        }
        if (pressed.found) {
            const tile = pressed.found;
            tile.dragging = false;
            this._carried = null;
            this._seamArea.queue_draw();
            const [x, y] = this._dragged(tile, dx, dy);
            this._dropFound(tile, x + tile.size[0] / 2, y + tile.size[1] / 2);
            return;
        }
        const drag = this._drag;
        drag.pixels = this._travel(drag, dx, dy);
        this._finishMove(drag);
    }

    _startMove({island, anchor, computer}) {
        const drag = this._drag = {
            owner: island.owner, anchor: {...anchor.monitor}, computer, fit: this._fitted,
            // How far the pointer went, in pixels.
            pixels: [0, 0],
            // The move the engine last answered for, and its answer: where
            // the screens land, or null where they cannot go.
            asked: null, landing: undefined, dropped: false,
        };
        // The moving screens draw over the others, under the names.
        for (const tile of this._tiles.values()) {
            if (this._moves(drag, tile)) tile.button.insert_before(this._board, null);
        }
        this._raiseChips();
        this._select(island.owner);
    }

    _moves(drag, tile) {
        return drag.computer ? tile.owner === drag.owner : tile.monitor.id === drag.anchor.id;
    }

    // As far as the pointer went, but with the screens kept on the board.
    _travel(drag, dx, dy) {
        const moving = [...this._tiles.values()].filter(tile => this._moves(drag, tile));
        if (!moving.length) return [dx, dy];
        const [width, height] = this._boardSize;
        const left = Math.min(...moving.map(tile => tile.x));
        const top = Math.min(...moving.map(tile => tile.y));
        const right = Math.max(...moving.map(tile => tile.x + tile.size[0]));
        const bottom = Math.max(...moving.map(tile => tile.y + tile.size[1]));
        const clamp = (value, min, max) => Math.min(Math.max(value, min), Math.max(min, max));
        return [clamp(dx, -left, width - right), clamp(dy, -top, height - bottom)];
    }

    _dragRequest(drag, command) {
        const {scale} = drag.fit;
        const {id, x, y} = drag.anchor;
        return {
            command, id, x: x + Math.round(drag.pixels[0] / scale), y: y + Math.round(drag.pixels[1] / scale),
            tolerance: Math.round(DRAG_TOLERANCE / scale), computer: drag.computer,
        };
    }

    // How far the moving screens show from their spot, in pixels: where
    // the engine says they land, else under the pointer.
    _shift(drag) {
        const {landing, anchor, fit: {scale}} = drag;
        return landing ? [(landing.x - anchor.x) * scale, (landing.y - anchor.y) * scale] : drag.pixels;
    }

    // Asks the engine where the drag lands, one question at a time, the
    // newest travel next.
    async _ask() {
        const drag = this._drag;
        if (!drag || drag.dropped || this._asking) return;
        const request = this._dragRequest(drag, 'preview');
        const asked = `${request.x},${request.y}`;
        if (drag.asked === asked) return;
        this._asking = true;
        let landing;
        try { landing = await this.client.call(request); }
        // Without an answer, the screens follow the pointer.
        catch { landing = undefined; }
        finally { this._asking = false; }
        if (this._disposed) return;
        if (this._drag === drag && !drag.dropped) {
            drag.asked = landing === undefined ? null : asked;
            drag.landing = landing;
            this._placeTiles();
        }
        if (landing !== undefined) this._ask();
    }

    // The screens stay where they show until the snapshot moves them, or
    // go back if the engine refuses.
    async _finishMove(drag) {
        drag.dropped = true;
        this._placeTiles();
        await this._run(this._dragRequest(drag, 'move_tile'));
        if (this._drag === drag) this._drag = null;
        if (!this._disposed) this._placeTiles();
    }

    // Where a carried found computer shows: it follows the pointer from the
    // shelf onto the board, but stays in the box.
    _dragged(tile, dx, dy) {
        const [width, height] = this._boardSize;
        const clamp = (value, max) => Math.min(Math.max(value, 0), Math.max(max, 0));
        return [clamp(tile.x + dx, width - tile.size[0]), clamp(tile.y + dy, height + this._shelfHeight - tile.size[1])];
    }

    // A found computer let go with its middle at (x, y) on the board joins
    // the layout there, with a new tile's size. Let go on the shelf, it
    // goes back.
    async _dropFound(tile, x, y) {
        if (y >= this._boardSize[1] || !this._origin) {
            this._placeShelf();
            return;
        }
        const {left, top, offsetX, offsetY} = this._origin;
        const scale = this._scale;
        const middle = [left + (x - offsetX) / scale, top + (y - offsetY) / scale];
        await this._place(tile, Math.round(middle[0] - NEW_TILE[0] / 2), Math.round(middle[1] - NEW_TILE[1] / 2), Math.round(DRAG_TOLERANCE / scale));
    }

    // Keyboard people put a found computer beside this one.
    _placeBeside(tile) {
        const local = [...this._tiles.values()].find(({monitor}) => !monitor.peer)?.monitor;
        if (!local || !PLACEABLE.has(tile.found.state)) return;
        this._place(tile, local.x + local.width, local.y, PLACE_TOLERANCE);
    }

    async _place(tile, x, y, tolerance) {
        const placed = await this._run({command: 'place', id: tile.found.id, x, y, tolerance});
        // A refused one goes back to the shelf; a placed one leaves it with
        // the next snapshot.
        if (!this._disposed) this._placeShelf();
        return placed;
    }

    _updateFound(found) {
        const key = JSON.stringify(found);
        if (key === this._foundKey) return;
        this._foundKey = key;
        const ids = new Set(found.map(({id}) => id));
        for (const [id, tile] of this._found) {
            if (ids.has(id)) continue;
            if (this._pressed?.found === tile) this._pressed = null;
            this._board.remove(tile.button);
            this._found.delete(id);
        }
        for (const computer of found) {
            const tile = this._found.get(computer.id) ?? this._newFound(computer);
            tile.found = computer;
            const placeable = PLACEABLE.has(computer.state);
            tile.name.label = tile.button.tooltip_text = computer.name;
            tile.detail.label = {
                identifying: 'Looking it up…',
                different_version: 'Different zflow version',
                duplicate_name: 'Same name as another',
            }[computer.state] ?? OS_NAMES[computer.os] ?? '';
            showMark(tile.mark, computer.mark ?? null);
            tile.button.sensitive = placeable;
            tile.button.update_property([Gtk.AccessibleProperty.DESCRIPTION], [placeable
                ? 'Found on your network. Drag it next to a screen, or press Enter to put it beside this computer, to add it.'
                : `Found on your network. ${tile.detail.label}.`]);
        }
        this._placeShelf();
    }

    _newFound(computer) {
        const name = new Gtk.Label({ellipsize: Pango.EllipsizeMode.END, max_width_chars: 14, xalign: 0, css_classes: ['heading']});
        const detail = new Gtk.Label({ellipsize: Pango.EllipsizeMode.END, max_width_chars: 16, xalign: 0, css_classes: ['caption', 'dim-label']});
        const mark = new Gtk.Box({spacing: 2, hexpand: true, halign: Gtk.Align.END});
        const top = new Gtk.Box();
        top.append(new Gtk.Image({icon_name: 'computer-symbolic'}));
        top.append(mark);
        const content = new Gtk.Box({orientation: Gtk.Orientation.VERTICAL, spacing: 2});
        content.append(top);
        content.append(name);
        content.append(detail);
        const button = new Gtk.Button({child: content});
        const tile = {button, name, detail, mark, found: computer, x: 0, y: 0, size: FOUND_SIZE, dragging: false};
        // Only a drag or a key places it, never a stray click.
        const keys = new Gtk.EventControllerKey();
        keys.connect('key-pressed', (_keys, keyval) => {
            if (![Gdk.KEY_Return, Gdk.KEY_KP_Enter, Gdk.KEY_space].includes(keyval)) return false;
            this._placeBeside(tile);
            return true;
        });
        button.add_controller(keys);
        this._board.put(button, 0, 0);
        this._found.set(computer.id, tile);
        return tile;
    }

    // Lays the found computers out in rows under the board.
    _placeShelf() {
        const [width, height] = this._boardSize;
        const tiles = [...this._found.values()];
        const perRow = Math.max(1, Math.floor((width - 2 * SHELF_GAP - 16) / (FOUND_SIZE[0] + SHELF_GAP)));
        const rows = Math.ceil(tiles.length / perRow);
        const shelfHeight = tiles.length ? SHELF_TOP + rows * (FOUND_SIZE[1] + SHELF_GAP) + SHELF_GAP : 0;
        if (shelfHeight !== this._shelfHeight) {
            this._shelfHeight = shelfHeight;
            this._area.content_height = BOARD_HEIGHT + shelfHeight;
        }
        this._shelf.visible = tiles.length > 0;
        this._shelf.set_size_request(Math.max(width - 2 * SHELF_GAP, 1), Math.max(shelfHeight - SHELF_GAP, 1));
        this._board.move(this._shelf, SHELF_GAP, height);
        tiles.forEach((tile, index) => {
            tile.x = 2 * SHELF_GAP + (index % perRow) * (FOUND_SIZE[0] + SHELF_GAP);
            tile.y = height + SHELF_TOP + Math.floor(index / perRow) * (FOUND_SIZE[1] + SHELF_GAP);
            tile.button.set_size_request(...FOUND_SIZE);
            if (!tile.dragging) this._board.move(tile.button, tile.x, tile.y);
        });
    }

    // Screens stay sensitive during a request, so the focused one keeps
    // focus; a move while another runs is dropped.
    async _moveTile(id, x, y, tolerance, computer) {
        const moved = await this._run({command: 'move_tile', id, x, y, tolerance, computer});
        if (!this._disposed) this._placeTiles();
        return moved;
    }

    // The computers on the arrangement, in its order, then any paired one
    // with no screen there.
    _computerList() {
        const peers = new Map(this._peers.map(peer => [peer.name, peer]));
        const computers = [...this._islands.values()].map(({owner, name, tiles, seed}) => {
            const peer = tiles[0].monitor.peer ? peers.get(tiles[0].monitor.peer) ?? null : null;
            return {owner, name: name.label, peer, tiles, seed};
        });
        for (const peer of this._peers) {
            const owner = `peer:${peer.name}`;
            if (!this._islands.has(owner)) computers.push({owner, name: peer.name, peer, tiles: [], seed: peer.mark});
        }
        return computers;
    }

    // Where the pointer goes from this computer's screens to another's.
    _crossingsOf({tiles}) {
        const mine = new Set(tiles.map(({monitor}) => monitor.id));
        return this._seams.filter(seam => !seam.same_computer && mine.has(seam.a) !== mine.has(seam.b));
    }

    // Rows are rebuilt only when what they say changes, so keyboard focus
    // stays. A setting's change only moves its control, below.
    _updateInspector() {
        const computers = this._computerList();
        const owners = new Set(computers.map(({owner}) => owner));
        // A forgotten computer lets go of the selection.
        if (!owners.has(this._selection)) this._selection = null;
        if (!owners.has(this._adjusting)) this._adjusting = null;
        const selected = computers.find(({owner}) => owner === this._selection) ?? null;
        const crossings = selected ? this._crossingsOf(selected) : [];
        const shown = computers.map(({owner, name, peer, tiles, seed}) => [
            owner, name, seed, peer && [peer.state, peer.detail], tiles.map(({monitor}) => [monitor.id, monitor.label, monitor.display]),
        ]);
        const key = JSON.stringify([this._selection, shown, crossings, this._peers.length]);
        if (key !== this._inspectorKey) {
            this._inspectorKey = key;
            if (selected) this._showComputer(selected, crossings);
            else this._showComputers(computers);
        }
        this._updating = true;
        if (this._adjustRow) this._adjustRow.active = this._adjusting !== null && this._adjusting === this._selection;
        const peer = selected?.peer;
        if (peer && this._controls.has(peer.name)) {
            const keyboard = this._keyboards.get(peer.name);
            const control = this._controls.get(peer.name);
            const scroll = this._scrolls.get(peer.name);
            const lens = this._lensRows.get(peer.name);
            keyboard.selected = Math.max(0, KEYBOARD_MODES.indexOf(peer.keyboard));
            control.active = peer.allow_control;
            scroll.active = !!peer.reverse_scroll;
            lens.active = !!peer.share_screen;
            keyboard.sensitive = control.sensitive = scroll.sensitive = lens.sensitive = !this._busy;
        }
        this._updating = false;
    }

    // Replaces the rows `group` shows.
    _fill(group, rows) {
        for (const row of this._filled.get(group) ?? []) group.remove(row);
        for (const row of rows) group.add(row);
        this._filled.set(group, rows);
        return rows;
    }

    _showComputers(computers) {
        this._inspector.title = '';
        this._inspector.description = computers.length ? 'Select a computer to see where the pointer crosses, its screens, and its settings.' : null;
        this._inspector.header_suffix = null;
        this._adjustRow = null;
        this._keyboards.clear();
        this._controls.clear();
        this._scrolls.clear();
        this._lensRows.clear();
        const rows = computers.map(computer => {
            const row = new Adw.ActionRow({title: computer.name, subtitle: summary(computer), use_markup: false, activatable: true});
            row.add_prefix(new Gtk.Image({icon_name: 'computer-symbolic'}));
            if (computer.peer) row.add_suffix(new Gtk.Box({css_classes: dotClasses(computer.peer.state), valign: Gtk.Align.CENTER}));
            const mark = new Gtk.Box({spacing: 2, valign: Gtk.Align.CENTER});
            showMark(mark, computer.seed);
            row.add_suffix(mark);
            row.add_suffix(new Gtk.Image({icon_name: 'go-next-symbolic'}));
            row.connect('activated', () => this._select(computer.owner, true));
            return row;
        });
        if (!this._peers.length) rows.push(new Adw.ActionRow({title: 'No computers added yet', subtitle: 'Computers running zflow on this network show up under the arrangement. Drag one next to this computer.', subtitle_lines: 0}));
        this._inspectorRows = this._fill(this._inspector, rows);
        for (const group of [this._crossings, this._screens, this._peerSettings]) {
            this._fill(group, []);
            group.visible = false;
        }
    }

    _showComputer(computer, crossings) {
        const {owner, name, peer, tiles, seed} = computer;
        this._inspector.title = name;
        this._inspector.description = peer ? peer.detail : null;
        const header = new Gtk.Box({spacing: 6, valign: Gtk.Align.CENTER});
        // Its mark shows here rather than on the board.
        const mark = new Gtk.Box({spacing: 2, valign: Gtk.Align.CENTER, margin_end: 6});
        showMark(mark, seed);
        header.append(mark);
        if (peer) {
            // Forgetting puts it back on the shelf, so it takes no asking.
            const forget = new Gtk.Button({icon_name: 'user-trash-symbolic', tooltip_text: `Forget ${name}`, valign: Gtk.Align.CENTER, css_classes: ['flat']});
            forget.connect('clicked', () => this._run({command: 'forget', name}));
            header.append(forget);
        }
        const close = new Gtk.Button({icon_name: 'window-close-symbolic', tooltip_text: 'Show all computers', valign: Gtk.Align.CENTER, css_classes: ['flat']});
        close.connect('clicked', () => this._select(null));
        header.append(close);
        this._inspector.header_suffix = header;
        const rows = [];
        this._adjustRow = null;
        if (tiles.length > 1) {
            const adjust = this._adjustRow = new Adw.SwitchRow({title: 'Adjust Screens', subtitle: 'Move its screens one by one instead of together. Double-clicking it on the arrangement does too.'});
            adjust.connect('notify::active', () => {
                if (!this._updating) this._adjust(owner, adjust.active);
            });
            rows.push(adjust);
        }
        if (!peer && GLib.find_program_in_path('gnome-control-center')) {
            const displays = new Adw.ActionRow({title: 'Display Settings', subtitle: 'Arrange this computer’s own screens.'});
            displays.add_suffix(button('Open', () => this._openDisplays()));
            rows.push(displays);
        }
        this._inspectorRows = this._fill(this._inspector, rows);

        const mine = new Set(tiles.map(({monitor}) => monitor.id));
        this._crossings.description = !peer && tiles.length > 1 ? 'Between this computer’s own screens, the pointer follows GNOME’s display settings.' : null;
        this._fill(this._crossings, crossings.length
            ? crossings.map(seam => this._crossingRow(seam, mine))
            : [new Adw.ActionRow({title: 'None yet', subtitle: 'Move it until an edge touches another computer’s screen.', subtitle_lines: 0})]);
        this._crossings.visible = tiles.length > 0;
        this._fill(this._screens, tiles.map(({monitor}) => {
            const row = new Adw.ActionRow({title: monitor.display?.name ?? monitor.label, subtitle: screenDetail(monitor.display), use_markup: false});
            row.add_prefix(new Gtk.Image({icon_name: 'video-display-symbolic'}));
            return row;
        }));
        this._screens.visible = tiles.length > 0;

        this._keyboards.clear();
        this._controls.clear();
        this._scrolls.clear();
        this._lensRows.clear();
        this._peerSettings.visible = !!peer;
        if (!peer) {
            this._fill(this._peerSettings, []);
            return;
        }
        const control = new Adw.SwitchRow({title: 'Can control this computer'});
        control.connect('notify::active', () => {
            if (!this._updating) this._run({command: 'set_peer', name, allow_control: control.active});
        });
        this._controls.set(name, control);
        const keyboard = new Adw.ComboRow({title: `Keys from ${name}`, subtitle: 'How its keys act here, from the next time it takes control', model: Gtk.StringList.new(['Standard keys', 'PC key positions', 'Mac shortcuts'])});
        keyboard.connect('notify::selected', () => {
            if (!this._updating) this._run({command: 'set_peer', name, keyboard: KEYBOARD_MODES[keyboard.selected]});
        });
        this._keyboards.set(name, keyboard);
        const scroll = new Adw.SwitchRow({title: 'Reverse scrolling', subtitle: 'Turn its scrolling around here'});
        scroll.connect('notify::active', () => {
            if (!this._updating) this._run({command: 'set_peer', name, reverse_scroll: scroll.active});
        });
        this._scrolls.set(name, scroll);
        const lens = new Adw.SwitchRow({title: 'Can see this screen', subtitle: 'Through a lens around its pointer'});
        lens.connect('notify::active', () => {
            if (!this._updating) this._run({command: 'set_peer', name, share_screen: lens.active});
        });
        this._lensRows.set(name, lens);
        this._fill(this._peerSettings, [control, keyboard, scroll, lens]);
    }

    _crossingRow(seam, mine) {
        const here = mine.has(seam.a) ? seam.a : seam.b;
        const there = here === seam.a ? seam.b : seam.a;
        const side = seam.vertical ? (here === seam.a ? 'Right' : 'Left') : (here === seam.a ? 'Bottom' : 'Top');
        const screen = id => {
            const monitor = this._tiles.get(id)?.monitor;
            return monitor ? monitor.display?.name ?? monitor.label : 'a screen';
        };
        const other = this._tiles.get(there);
        const row = new Adw.ActionRow({
            title: other ? this._islands.get(other.owner)?.name.label ?? there : there,
            subtitle: `${side} edge of ${screen(here)} → ${screen(there)}`, use_markup: false,
        });
        row.add_prefix(badge(seam.vertical));
        return row;
    }

    _openDisplays() {
        try { Gio.Subprocess.new(['gnome-control-center', 'display'], Gio.SubprocessFlags.NONE); }
        catch (error) { this._showError(error.message); }
    }

    _updateShortcuts(shortcuts) {
        const key = JSON.stringify(shortcuts);
        if (key === this._shortcutKey) return;
        this._shortcutKey = key;
        for (const row of this._shortcutRows ?? []) this._shortcuts.remove(row);
        this._shortcutRows = shortcuts.map(({title, keys}) => {
            const row = new Adw.ActionRow({title, use_markup: false});
            row.add_suffix(new Gtk.Label({label: keys, css_classes: ['dim-label', 'monospace']}));
            this._shortcuts.add(row);
            return row;
        });
        this._shortcuts.visible = shortcuts.length > 0;
    }

    // For a computer mDNS cannot see, such as one on Tailscale. What answers
    // shows up on the shelf.
    _openAddAddress() {
        const dialog = new Adw.AlertDialog({heading: 'Add a Computer by Address', body: 'For a computer zflow can’t find on this network, like one on Tailscale. It shows up in Found on your network; drag it into place to add it.'});
        const entry = new Gtk.Entry({placeholder_text: 'IP address, like 100.64.0.7', activates_default: true, input_purpose: Gtk.InputPurpose.URL});
        dialog.extra_child = entry;
        dialog.add_response('cancel', 'Cancel');
        dialog.add_response('look_up', 'Look Up');
        dialog.set_response_appearance('look_up', Adw.ResponseAppearance.SUGGESTED);
        dialog.default_response = 'look_up';
        dialog.close_response = 'cancel';
        dialog.connect('response', (_dialog, response) => {
            if (response === 'look_up') this._run({command: 'add_address', address: entry.text.trim()});
        });
        this._addDialog = {dialog, entry};
        dialog.present(this.window);
    }

    destroy() {
        this._disposed = true;
        this.client.destroy();
    }
}
