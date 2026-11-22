# Showcase

Real screens composed from penna's components and blocks — the terminal multiplexer (`zz`) across desktop and phone, the multi-machine desk manager (`nidus`), and live GPU-to-HTML parity measured from every storybook baseline.

```figure uses=badge,chip,compact,dock,icon,input,inspector,kbd,label,list,notice,panel,progress,segmented,settings,sidebar,switch,tab,tile,title_bar,touch
<div class="docs-sc-toolbar" data-penna="card" data-ground data-docs-showcase>
  <div class="docs-sc-ctrl">
    <span class="docs-sc-ctrl-label">Look</span>
    <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Look" data-sc-theme>
      <div class="penna-segments" data-penna="button.quiet" data-state="current">
        <span class="penna-segment-pill"></span>
        <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Default</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>macOS</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Yaru</span></span>
      </div>
    </div>
  </div>
  <div class="docs-sc-ctrl">
    <span class="docs-sc-ctrl-label">Mode</span>
    <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Mode" data-sc-mode>
      <div class="penna-segments" data-penna="button.quiet" data-state="current">
        <span class="penna-segment-pill"></span>
        <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Dark</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Light</span></span>
      </div>
    </div>
  </div>
  <div class="docs-sc-ctrl">
    <span class="docs-sc-ctrl-label">Glass</span>
    <button class="penna-toggle" role="switch" data-size="small" aria-checked="true" aria-label="Liquid glass" type="button" data-sc-glass></button>
  </div>
</div>
```

## zz workspace

Switch sessions in the sidebar, toggle the split agent pane, send a message in the composer, or flip to the 390 px phone layout.

```figure uses=badge,chip,compact,icon,input,label,list,panel,segmented,sidebar,tab,tile,title_bar,touch
<div class="docs-sc-section" data-sc-zz>
  <div class="docs-sc-bar">
    <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="zz viewport" data-sc-zz-view>
      <div class="penna-segments" data-penna="button.quiet" data-state="current">
        <span class="penna-segment-pill"></span>
        <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Desktop</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Phone</span></span>
      </div>
    </div>
    <button class="penna-button" data-penna="button.quiet" data-size="small" type="button" data-sc-zz-split aria-pressed="true"><i data-icon="panel-right"></i><span class="penna-label" data-penna-text>Split chat</span></button>
  </div>

  <div class="docs-sc-zz-desktop" data-sc-zz-pane="desktop">
    <div class="penna-window docs-sc-win" data-ground>
      <aside class="penna-sidebar docs-sc-side" data-penna="sidebar" data-ground>
        <div class="penna-lights" aria-hidden="true"><i></i><i></i><i></i></div>
        <div class="penna-strip"><span class="penna-type" data-type="caption" data-muted data-penna-text>zz · studio</span></div>
        <div class="docs-sc-side-list" role="listbox" aria-label="Sessions">
          <button class="penna-button penna-sidebar-row" data-penna="sidebar.row" type="button" data-state="current" aria-current="true" data-sc-session="work">
            <i class="penna-dot" data-tone="success"></i>
            <span class="penna-label" data-penna-text>work</span>
            <span class="docs-sc-side-meta">3</span>
          </button>
          <button class="penna-button penna-sidebar-row" data-penna="sidebar.row" type="button" data-sc-session="build">
            <i class="penna-dot" data-tone="warning"></i>
            <span class="penna-label" data-penna-text>build</span>
            <span class="docs-sc-side-meta">1</span>
          </button>
          <button class="penna-button penna-sidebar-row" data-penna="sidebar.row" type="button" data-sc-session="agents">
            <i class="penna-dot" data-tone="accent"></i>
            <span class="penna-label" data-penna-text>agents</span>
            <span class="docs-sc-side-meta">2</span>
          </button>
          <button class="penna-button penna-sidebar-row" data-penna="sidebar.row" type="button" data-sc-session="logs">
            <i class="penna-dot"></i>
            <span class="penna-label" data-penna-text>logs</span>
            <span class="docs-sc-side-meta">4</span>
          </button>
        </div>
      </aside>
      <header class="docs-sc-titlebar" data-penna="titlebar">
        <div class="docs-sc-tb-left">
          <span class="penna-type" data-type="small" data-strong data-sc-zz-title>work</span>
          <span class="penna-badge" data-penna="badge"><i data-icon="git-branch"></i><span class="badge-text" data-sc-zz-branch>main</span></span>
        </div>
        <div class="docs-sc-tb-right">
          <span class="penna-chip" data-penna="chip" data-size="xsmall"><i class="penna-dot" data-tone="success" data-sc-zz-dot></i><span class="penna-chip-label" data-sc-zz-status>Running</span></span>
        </div>
      </header>
      <div class="docs-sc-stage" data-penna="pane" data-split="true" data-sc-zz-stage>
        <div class="docs-sc-term" data-penna="card.code" data-ground>
          <div class="docs-sc-term-bar">
            <span class="docs-sc-term-tab" data-sc-zz-cmd>cargo test --workspace</span>
            <span class="docs-lk-mono" data-sc-zz-cwd>~/dev/penna</span>
          </div>
          <pre class="docs-sc-term-out" data-sc-zz-out></pre>
        </div>
        <div class="docs-sc-chat" data-penna="panel" data-ground data-sc-zz-chat>
          <div class="docs-sc-chat-head">
            <span class="penna-type" data-type="small" data-strong data-penna-text>Agent</span>
            <span class="penna-badge" data-penna="badge" data-tone="success"><span class="badge-text" data-penna-text>Synced</span></span>
          </div>
          <div class="docs-sc-chat-feed" data-sc-zz-feed>
            <div class="docs-sc-msg" data-role="assistant">
              <span class="docs-sc-msg-tool" data-penna="card" data-ground><i data-icon="circle-check" data-tone="success"></i><span>cargo test · 473 passed</span></span>
              <span class="docs-sc-msg-text">Parity baselines verified across 34 stories. Ready for the next check.</span>
            </div>
          </div>
          <form class="docs-sc-composer" data-sc-zz-form>
            <label class="penna-input" data-penna="field" data-size="small"><input type="text" placeholder="Ask agent…" aria-label="Ask agent" data-sc-zz-input /></label>
            <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="small" data-shape="icon" type="submit" aria-label="Send"><i data-icon="arrow-up"></i></button>
          </form>
        </div>
      </div>
    </div>
  </div>

  <div class="docs-sc-zz-phone" data-sc-zz-pane="phone" hidden>
    <div class="docs-phone docs-sc-phone-frame">
      <div class="docs-sc-phone-top">
        <span class="penna-type" data-type="title" data-penna-text>Machines</span>
        <span class="penna-chip" data-penna="chip" data-size="xsmall"><i class="penna-dot" data-tone="success"></i><span class="penna-chip-label" data-penna-text>2 online</span></span>
      </div>
      <div class="docs-sc-phone-tiles">
        <div class="penna-tile docs-sc-ptile" data-penna="tile.card" data-variant="card" data-state="selected">
          <div class="docs-sc-ptile-top"><i data-icon="laptop"></i><i class="penna-dot" data-tone="success"></i></div>
          <span class="penna-type" data-type="body" data-strong data-penna-text>macbook</span>
          <span class="penna-type" data-type="caption" data-muted data-penna-text>work · 3 panes</span>
        </div>
        <div class="penna-tile docs-sc-ptile" data-penna="tile.card" data-variant="card">
          <div class="docs-sc-ptile-top"><i data-icon="hard-drive"></i><i class="penna-dot" data-tone="warning"></i></div>
          <span class="penna-type" data-type="body" data-strong data-penna-text>alienware</span>
          <span class="penna-type" data-type="caption" data-muted data-penna-text>build · compiling</span>
        </div>
      </div>
      <div class="penna-compact-bar docs-sc-phone-bar">
        <div class="penna-compact-pill" data-penna="phone.pill" data-ground>
          <div class="penna-pill-tile" data-penna="phone.pill.icon"><i data-icon="square-terminal"></i></div>
          <div class="penna-pill-copy">
            <span class="penna-pill-title">macbook · work</span>
            <span class="penna-pill-meta"><i class="penna-dot" data-tone="success"></i><span class="penna-pill-meta-text">473 tests passing</span></span>
          </div>
        </div>
        <button class="penna-touch-disc" data-penna="floating" type="button" aria-label="New pane"><i data-icon="plus"></i></button>
      </div>
    </div>
  </div>
</div>
```

## nidus desk

Click any display tile on the arrangement canvas or a nearby host in the dock to inspect and toggle its live permissions.

```figure uses=chip,dock,icon,inspector,label,settings,switch,tile
<div class="docs-sc-nidus" data-sc-nidus>
  <div class="docs-sc-nidus-main">
    <div class="docs-sc-desk">
      <div class="docs-sc-desk-label"><span class="penna-type" data-type="caption" data-muted data-penna-text>Desk arrangement · QUIC mesh</span></div>
      <div class="docs-sc-desk-stage">
        <button class="penna-tile docs-sc-screen" data-penna="tile" data-variant="screen" type="button" data-sc-host="ubuntu">
          <span class="docs-sc-screen-name">ubuntu</span>
          <span class="docs-sc-screen-sub">27″ · 4K</span>
        </button>
        <button class="penna-tile docs-sc-screen docs-sc-screen-primary" data-penna="tile" data-variant="screen" data-state="selected" type="button" data-sc-host="macbook">
          <span class="docs-sc-screen-name">macbook</span>
          <span class="docs-sc-screen-sub">16″ · Primary</span>
        </button>
        <button class="penna-tile docs-sc-screen docs-sc-screen-sm" data-penna="tile" data-variant="screen" type="button" data-sc-host="thinkpad">
          <span class="docs-sc-screen-name">thinkpad</span>
          <span class="docs-sc-screen-sub">14″ · Linux</span>
        </button>
      </div>
    </div>
    <aside class="penna-inspector docs-sc-inspector" data-penna="inspector" data-ground>
      <div class="penna-inspector-header">
        <div class="penna-inspector-titles">
          <span class="penna-type" data-type="heading" data-truncate data-sc-nidus-name>macbook</span>
          <span class="penna-type" data-type="small" data-muted data-sc-nidus-sub>macOS · Primary · 1.2 ms</span>
        </div>
        <span class="penna-chip" data-penna="chip" data-size="xsmall"><i class="penna-dot" data-tone="success" data-sc-nidus-dot></i><span class="penna-chip-label" data-sc-nidus-state>Online</span></span>
      </div>
      <div class="penna-settings-stack">
        <div class="penna-settings-group"><span data-penna-text>Permissions</span></div>
        <div class="penna-settings-list" role="list">
          <div class="penna-setting-entry" role="listitem">
            <div data-penna="settings.card" data-ground>
              <div class="penna-setting-body">
                <div class="penna-setting-heading">
                  <div class="penna-setting-copy"><div class="penna-setting-title"><span data-penna-text>Pointer &amp; keyboard</span></div></div>
                  <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Pointer and keyboard" type="button" data-sc-perm="pointer"></button>
                </div>
              </div>
            </div>
          </div>
          <div class="penna-setting-entry" role="listitem">
            <div data-penna="settings.card" data-ground>
              <div class="penna-setting-rule"><div data-penna="rule.list"></div></div>
              <div class="penna-setting-body">
                <div class="penna-setting-heading">
                  <div class="penna-setting-copy"><div class="penna-setting-title"><span data-penna-text>Clipboard sync</span></div></div>
                  <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Clipboard sync" type="button" data-sc-perm="clipboard"></button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
      <div class="docs-sc-nidus-meta">
        <span class="penna-type" data-type="small" data-strong data-penna-text>Displays</span>
        <span class="penna-type" data-type="small" data-muted data-sc-nidus-displays>Built-in Liquid Retina 3456×2234</span>
      </div>
    </aside>
  </div>
  <div class="penna-dock docs-sc-dock">
    <div class="penna-dock-header">
      <div class="penna-dock-titles">
        <span class="penna-type" data-type="body" data-strong data-penna-text>Nearby</span>
        <span class="penna-type" data-type="small" data-muted>Discovered on local network via mDNS.</span>
      </div>
      <span class="penna-badge" data-penna="badge"><span class="badge-text" data-penna-text>2 peers</span></span>
    </div>
    <div class="penna-dock-tiles">
      <button class="penna-tile" data-penna="tile.card" data-variant="card" type="button" data-sc-host="xps"><span class="penna-type" data-type="body" data-strong data-truncate data-penna-text>xps</span><span class="penna-type" data-type="caption" data-truncate data-penna-text>Linux · 1 screen</span></button>
      <button class="penna-tile" data-penna="tile.card" data-variant="card" type="button" data-sc-host="studio"><span class="penna-type" data-type="body" data-strong data-truncate data-penna-text>studio</span><span class="penna-type" data-type="caption" data-truncate data-penna-text>macOS · 2 screens</span></button>
    </div>
  </div>
</div>
```

## Parity

Every component and block twin is rendered in both native `ala` GPU and headless Chromium across all six `theme × mode` pairs (`default`, `macos`, `yaru` in dark and light) and scored pixel-for-pixel. The table below loads live from `assets/parity.json`, generated directly from `storybook/src/kit/*/parity.baseline.json`.

```figure uses=badge,input,progress,segmented
<div class="docs-sc-parity" data-sc-parity>
  <div class="docs-sc-kpis" data-sc-parity-kpis></div>
  <div class="docs-sc-parity-bar">
    <label class="penna-input docs-sc-parity-search" data-penna="field" data-size="small"><i data-icon="search"></i><input type="search" placeholder="Filter 34 stories…" aria-label="Filter parity stories" data-sc-parity-filter /></label>
    <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Sort parity" data-sc-parity-sort>
      <div class="penna-segments" data-penna="button.quiet" data-state="current">
        <span class="penna-segment-pill"></span>
        <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Name</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Highest</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Lowest</span></span>
      </div>
    </div>
  </div>
  <div class="docs-sc-parity-list" data-penna="card" data-ground data-sc-parity-list></div>
</div>
```
