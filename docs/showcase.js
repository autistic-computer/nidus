(() => {
  if (!document.querySelector("[data-docs-showcase]")) return;

  const THEMES = ["default", "macos", "yaru"];
  const MODES = ["dark", "light"];

  const themeSeg = document.querySelector("[data-sc-theme]");
  const modeSeg = document.querySelector("[data-sc-mode]");
  const glassToggle = document.querySelector("[data-sc-glass]");

  function syncToolbar() {
    const themeIdx = Math.max(0, THEMES.indexOf(pennaKit.choice.theme));
    const modeIdx = Math.max(0, MODES.indexOf(pennaKit.choice.mode));
    const glassOn = document.documentElement.dataset.glass !== "none";
    if (themeSeg) {
      themeSeg.dataset.value = String(themeIdx);
      window.pennaSegmented?.set?.(themeSeg, themeIdx);
    }
    if (modeSeg) {
      modeSeg.dataset.value = String(modeIdx);
      window.pennaSegmented?.set?.(modeSeg, modeIdx);
    }
    if (glassToggle) {
      glassToggle.setAttribute("aria-checked", String(glassOn));
    }
  }

  themeSeg?.addEventListener("change", (event) => {
    const next = THEMES[event.detail.index];
    if (next) pennaKit.setTheme(next);
  });

  modeSeg?.addEventListener("change", (event) => {
    const next = MODES[event.detail.index];
    if (next && next !== pennaKit.choice.mode) {
      pennaKit.choice.mode = next;
      pennaKit.apply();
    }
  });

  glassToggle?.addEventListener("change", (event) => {
    pennaKit.setGlass(Boolean(event.detail.checked));
  });

  const SESSIONS = {
    work: {
      title: "work",
      branch: "main",
      status: "Running",
      tone: "success",
      cmd: "cargo test --workspace",
      cwd: "~/dev/penna",
      out: "   Compiling penna v0.4.0\n    Finished test [unoptimized] in 1.84s\n     Running unittests src/lib.rs\ntest result: ok. 473 passed; 0 failed; 0 ignored",
    },
    build: {
      title: "build",
      branch: "feat/tokens",
      status: "Building",
      tone: "warning",
      cmd: "cargo run --example docs -- --check",
      cwd: "~/dev/penna",
      out: "generating tokens for default, macos, yaru…\nwriting 67 pages + llms.txt + parity.json\ndocs check ok: 67 pages across 4 groups, 107 icons verified",
    },
    agents: {
      title: "agents",
      branch: "main",
      status: "Active",
      tone: "accent",
      cmd: "zz attach studio/agents",
      cwd: "~/dev/zebrilus",
      out: "[pane 1] watching src/foundation/motion.rs\n[pane 2] parity diff: 34 stories · 3,084 checks\n[agent]  all surface roles matched baseline",
    },
    logs: {
      title: "logs",
      branch: "main",
      status: "Idle",
      tone: "",
      cmd: "tail -f /tmp/zz-daemon.log",
      cwd: "/var/log/zz",
      out: "19:02:11 quic: peer ubuntu connected (1.2 ms)\n19:02:14 clipboard: synced 1.4 KiB text/plain\n19:02:18 focus: pane work:0 active",
    },
  };

  const zzRoot = document.querySelector("[data-sc-zz]");
  if (zzRoot) {
    const viewSeg = zzRoot.querySelector("[data-sc-zz-view]");
    const splitBtn = zzRoot.querySelector("[data-sc-zz-split]");
    const stage = zzRoot.querySelector("[data-sc-zz-stage]");
    const panes = [...zzRoot.querySelectorAll("[data-sc-zz-pane]")];
    const sessionButtons = [...zzRoot.querySelectorAll("[data-sc-session]")];
    const titleEl = zzRoot.querySelector("[data-sc-zz-title]");
    const branchEl = zzRoot.querySelector("[data-sc-zz-branch]");
    const statusEl = zzRoot.querySelector("[data-sc-zz-status]");
    const dotEl = zzRoot.querySelector("[data-sc-zz-dot]");
    const cmdEl = zzRoot.querySelector("[data-sc-zz-cmd]");
    const cwdEl = zzRoot.querySelector("[data-sc-zz-cwd]");
    const outEl = zzRoot.querySelector("[data-sc-zz-out]");
    const form = zzRoot.querySelector("[data-sc-zz-form]");
    const input = zzRoot.querySelector("[data-sc-zz-input]");
    const feed = zzRoot.querySelector("[data-sc-zz-feed]");

    function selectSession(key) {
      const data = SESSIONS[key] || SESSIONS.work;
      sessionButtons.forEach((btn) => {
        const active = btn.dataset.scSession === key;
        if (active) {
          btn.dataset.state = "current";
          btn.setAttribute("aria-current", "true");
        } else {
          delete btn.dataset.state;
          btn.removeAttribute("aria-current");
        }
      });
      if (titleEl) titleEl.textContent = data.title;
      if (branchEl) branchEl.textContent = data.branch;
      if (statusEl) statusEl.textContent = data.status;
      if (dotEl) {
        if (data.tone) dotEl.dataset.tone = data.tone;
        else delete dotEl.dataset.tone;
      }
      if (cmdEl) cmdEl.textContent = data.cmd;
      if (cwdEl) cwdEl.textContent = data.cwd;
      if (outEl) outEl.textContent = data.out;
    }

    sessionButtons.forEach((btn) => {
      btn.addEventListener("click", () => selectSession(btn.dataset.scSession));
    });
    selectSession("work");

    viewSeg?.addEventListener("change", (event) => {
      const target = event.detail.index === 1 ? "phone" : "desktop";
      panes.forEach((pane) => {
        pane.hidden = pane.dataset.scZzPane !== target;
      });
      if (splitBtn) splitBtn.hidden = target === "phone";
      requestAnimationFrame(() => window.pennaKit?.roundAll?.(zzRoot));
    });

    splitBtn?.addEventListener("click", () => {
      if (!stage) return;
      const next = stage.dataset.split !== "true";
      stage.dataset.split = String(next);
      splitBtn.setAttribute("aria-pressed", String(next));
      requestAnimationFrame(() => window.pennaKit?.roundAll?.(stage));
    });

    form?.addEventListener("submit", (event) => {
      event.preventDefault();
      const text = input?.value.trim();
      if (!text || !feed) return;
      const mine = document.createElement("div");
      mine.className = "docs-sc-msg";
      mine.dataset.role = "user";
      mine.textContent = text;
      const reply = document.createElement("div");
      reply.className = "docs-sc-msg";
      reply.dataset.role = "assistant";
      const replyText = document.createElement("span");
      replyText.className = "docs-sc-msg-text";
      replyText.textContent = `Queued "${text}" in ${titleEl?.textContent || "work"}.`;
      reply.append(replyText);
      feed.append(mine, reply);
      feed.scrollTop = feed.scrollHeight;
      input.value = "";
    });
  }

  const HOSTS = {
    macbook: {
      name: "macbook",
      sub: "macOS · Primary · 1.2 ms",
      state: "Online",
      tone: "success",
      displays: "Built-in Liquid Retina 3456×2234",
      pointer: true,
      clipboard: true,
    },
    ubuntu: {
      name: "ubuntu",
      sub: "Linux · Left edge · 0.9 ms",
      state: "Online",
      tone: "success",
      displays: "DP-1 27″ 3840×2160 · HDMI-1 24″",
      pointer: true,
      clipboard: true,
    },
    thinkpad: {
      name: "thinkpad",
      sub: "Linux · Right edge · 2.4 ms",
      state: "Online",
      tone: "success",
      displays: "eDP-1 14″ 2880×1800",
      pointer: true,
      clipboard: false,
    },
    xps: {
      name: "xps",
      sub: "Linux · Nearby peer · mDNS",
      state: "Ready",
      tone: "warning",
      displays: "1 display · drag onto desk to pair",
      pointer: false,
      clipboard: true,
    },
    studio: {
      name: "studio",
      sub: "macOS · Nearby peer · mDNS",
      state: "Ready",
      tone: "warning",
      displays: "2 displays · Studio Display + Pro Display",
      pointer: false,
      clipboard: false,
    },
  };

  const nidusRoot = document.querySelector("[data-sc-nidus]");
  if (nidusRoot) {
    let activeHost = "macbook";
    const hostButtons = [...nidusRoot.querySelectorAll("[data-sc-host]")];
    const nameEl = nidusRoot.querySelector("[data-sc-nidus-name]");
    const subEl = nidusRoot.querySelector("[data-sc-nidus-sub]");
    const stateEl = nidusRoot.querySelector("[data-sc-nidus-state]");
    const dotEl = nidusRoot.querySelector("[data-sc-nidus-dot]");
    const displaysEl = nidusRoot.querySelector("[data-sc-nidus-displays]");
    const permPointer = nidusRoot.querySelector('[data-sc-perm="pointer"]');
    const permClipboard = nidusRoot.querySelector('[data-sc-perm="clipboard"]');

    function selectHost(key) {
      activeHost = key;
      const host = HOSTS[key] || HOSTS.macbook;
      hostButtons.forEach((btn) => {
        if (btn.dataset.scHost === key) btn.dataset.state = "selected";
        else delete btn.dataset.state;
      });
      if (nameEl) nameEl.textContent = host.name;
      if (subEl) subEl.textContent = host.sub;
      if (stateEl) stateEl.textContent = host.state;
      if (dotEl) dotEl.dataset.tone = host.tone;
      if (displaysEl) displaysEl.textContent = host.displays;
      if (permPointer) permPointer.setAttribute("aria-checked", String(host.pointer));
      if (permClipboard) permClipboard.setAttribute("aria-checked", String(host.clipboard));
    }

    hostButtons.forEach((btn) => {
      btn.addEventListener("click", () => selectHost(btn.dataset.scHost));
    });
    permPointer?.addEventListener("change", (event) => {
      HOSTS[activeHost].pointer = Boolean(event.detail.checked);
    });
    permClipboard?.addEventListener("change", (event) => {
      HOSTS[activeHost].clipboard = Boolean(event.detail.checked);
    });
    selectHost("macbook");
  }

  const parityRoot = document.querySelector("[data-sc-parity]");
  if (parityRoot) {
    const kpisHost = parityRoot.querySelector("[data-sc-parity-kpis]");
    const listHost = parityRoot.querySelector("[data-sc-parity-list]");
    const filterInput = parityRoot.querySelector("[data-sc-parity-filter]");
    const sortSeg = parityRoot.querySelector("[data-sc-parity-sort]");
    let report = null;
    let sortMode = 0;

    const STORY_LINKS = {
      appearance: "appearance.html",
      buttons: "button.html",
      canvas: "dock.html",
      chat: "chat.html",
      checks: "check.html",
      code_editor: "code-editor.html",
      color_picker: "color-picker.html",
      compact: "zoom-card.html",
      dialogs: "dialog.html",
      display: "badge.html",
      feedback: "empty-state.html",
      icons: "icon.html",
      inputs: "input.html",
      key_field: "key-field.html",
      markdown: "text.html",
      menus: "menu.html",
      navigation: "sidebar.html",
      notifications: "notification.html",
      palette: "palette.html",
      panels: "panel.html",
      phone: "phone.html",
      rows: "row.html",
      scrollbars: "scroll.html",
      selection: "select.html",
      settings: "settings.html",
      sheet: "sheet.html",
      stack_menu: "stack-menu.html",
      status: "notice.html",
      title_bar: "title-bar.html",
      which_key: "which-key.html",
    };

    function renderKpis(data) {
      if (!kpisHost) return;
      const above95 = data.stories.filter((s) => s.mean >= 95).length;
      const cards = [
        { label: "Overall parity", value: `${data.overall_mean.toFixed(2)}%` },
        { label: "Baseline checks", value: data.total_checks.toLocaleString() },
        { label: "Stories", value: String(data.story_count) },
        { label: "95%+ mean", value: `${above95} / ${data.story_count}` },
      ];
      kpisHost.replaceChildren(
        ...cards.map((item) => {
          const card = document.createElement("div");
          card.className = "docs-sc-kpi";
          card.setAttribute("data-penna", "card");
          card.setAttribute("data-ground", "");
          const val = document.createElement("span");
          val.className = "docs-sc-kpi-val";
          val.textContent = item.value;
          const lbl = document.createElement("span");
          lbl.className = "docs-sc-kpi-label";
          lbl.textContent = item.label;
          card.append(val, lbl);
          return card;
        }),
      );
    }

    function renderList() {
      if (!report || !listHost) return;
      const q = (filterInput?.value || "").trim().toLowerCase();
      let rows = report.stories.filter((s) => !q || s.story.toLowerCase().includes(q));
      if (sortMode === 1) rows = [...rows].sort((a, b) => b.mean - a.mean);
      else if (sortMode === 2) rows = [...rows].sort((a, b) => a.mean - b.mean);
      const frag = document.createDocumentFragment();
      for (const item of rows) {
        const href = STORY_LINKS[item.story];
        const row = document.createElement(href ? "a" : "div");
        row.className = "docs-sc-prow";
        if (href) row.href = href;
        const name = document.createElement("span");
        name.className = "docs-sc-pname";
        name.textContent = item.story.replaceAll("_", "-");
        const barWrap = document.createElement("span");
        barWrap.className = "docs-sc-pbar";
        const prog = document.createElement("div");
        prog.className = "penna-progress";
        prog.dataset.size = "small";
        const fill = document.createElement("div");
        fill.className = "penna-progress-fill";
        fill.style.setProperty("--penna-value", `${Math.min(100, Math.max(0, item.mean))}%`);
        if (item.mean >= 95) fill.style.background = "var(--penna-success)";
        else if (item.mean < 85) fill.style.background = "var(--penna-warning)";
        prog.append(fill);
        barWrap.append(prog);
        const checks = document.createElement("span");
        checks.className = "docs-sc-pmeta";
        checks.textContent = `${item.checks} checks`;
        const score = document.createElement("span");
        score.className = "docs-sc-pscore";
        score.textContent = `${item.mean.toFixed(1)}%`;
        row.append(name, barWrap, checks, score);
        frag.append(row);
      }
      if (!rows.length) {
        const empty = document.createElement("div");
        empty.className = "docs-sc-pempty";
        empty.textContent = "No matching stories";
        frag.append(empty);
      }
      listHost.replaceChildren(frag);
    }

    filterInput?.addEventListener("input", renderList);
    sortSeg?.addEventListener("change", (event) => {
      sortMode = event.detail.index;
      renderList();
    });

    fetch("assets/parity.json")
      .then((res) => (res.ok ? res.json() : null))
      .then((data) => {
        if (!data) return;
        report = data;
        renderKpis(data);
        renderList();
        requestAnimationFrame(() => window.pennaKit?.roundAll?.(parityRoot));
      })
      .catch(() => {});
  }

  document.addEventListener("penna-theme", syncToolbar);
  syncToolbar();
  requestAnimationFrame(() => {
    window.pennaKit?.roundAll?.(document);
    window.pennaSegmented?.wire?.(document);
  });
})();
