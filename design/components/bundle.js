(function () {
  var GAPS = { tight: "gap-tight", related: "gap-related", group: "gap-group", section: "gap-section" };
  var INSETS = { row: "inset-row", chrome: "inset-chrome", card: "inset-card", pane: "inset-pane", page: "inset-page" };

  function gapToken(value) {
    return GAPS[value] || (/^\d+$/.test(value) ? "space-" + value : value);
  }

  function insetToken(value) {
    return INSETS[value] || (/^\d+$/.test(value) ? "space-" + value : value);
  }

  function num(value) {
    var n = parseFloat(value);
    return isNaN(n) ? 0 : n;
  }

  function fmt(n) {
    return String(Math.round(n * 100) / 100);
  }

  function layerOf(root) {
    var layer = root.querySelector(":scope > .pd-redlines");
    if (!layer) {
      layer = document.createElement("div");
      layer.className = "pd-redlines";
      root.appendChild(layer);
    }
    return layer;
  }

  function band(layer, origin, box, kind, token, owner) {
    if (box.w < 0.5 || box.h < 0.5) return false;
    var el = document.createElement("div");
    el.className = "pd-rl";
    el.dataset.kind = kind;
    el.dataset.token = token;
    el.dataset.owner = owner;
    el.style.left = box.x - origin.left + "px";
    el.style.top = box.y - origin.top + "px";
    el.style.width = box.w + "px";
    el.style.height = box.h + "px";
    layer.appendChild(el);
    return true;
  }

  function children(el) {
    return Array.prototype.filter.call(el.children, function (child) {
      if (child.classList.contains("pd-redlines")) return false;
      var style = getComputedStyle(child);
      if (style.display === "none" || style.position === "absolute" || style.position === "fixed") return false;
      var box = child.getBoundingClientRect();
      return box.width > 0 || box.height > 0;
    });
  }

  function spacer(el) {
    return el.classList.contains("pd-spacer");
  }

  function gaps(layer, origin, el, owner, seen) {
    var style = getComputedStyle(el);
    var row = num(style.rowGap);
    var col = num(style.columnGap);
    if (!row && !col) return;
    var token = gapToken(el.dataset.gap);
    var list = children(el);
    for (var i = 1; i < list.length; i++) {
      if (spacer(list[i - 1]) || spacer(list[i])) continue;
      var a = list[i - 1].getBoundingClientRect();
      var b = list[i].getBoundingClientRect();
      var box = null;
      var size = 0;
      if (col && b.left >= a.right - 0.5 && b.top < a.bottom && b.bottom > a.top) {
        var top = Math.min(a.top, b.top);
        box = { x: a.right, y: top, w: b.left - a.right, h: Math.max(a.bottom, b.bottom) - top };
        size = box.w;
      } else if (row && b.top >= a.bottom - 0.5) {
        var left = Math.min(a.left, b.left);
        box = { x: left, y: a.bottom, w: Math.max(a.right, b.right) - left, h: b.top - a.bottom };
        size = box.h;
      }
      if (box && band(layer, origin, box, "gap", token, owner)) seen[token] = { kind: "gap", value: size };
    }
  }

  function inset(layer, origin, el, owner, seen) {
    var style = getComputedStyle(el);
    var r = el.getBoundingClientRect();
    var pt = num(style.paddingTop);
    var pr = num(style.paddingRight);
    var pb = num(style.paddingBottom);
    var pl = num(style.paddingLeft);
    if (!pt && !pr && !pb && !pl) return;
    var token = insetToken(el.dataset.inset);
    var x0 = r.left + num(style.borderLeftWidth);
    var y0 = r.top + num(style.borderTopWidth);
    var x1 = r.right - num(style.borderRightWidth);
    var y1 = r.bottom - num(style.borderBottomWidth);
    var any = false;
    any = band(layer, origin, { x: x0, y: y0, w: x1 - x0, h: pt }, "inset", token, owner) || any;
    any = band(layer, origin, { x: x0, y: y1 - pb, w: x1 - x0, h: pb }, "inset", token, owner) || any;
    any = band(layer, origin, { x: x0, y: y0 + pt, w: pl, h: y1 - y0 - pt - pb }, "inset", token, owner) || any;
    any = band(layer, origin, { x: x1 - pr, y: y0 + pt, w: pr, h: y1 - y0 - pt - pb }, "inset", token, owner) || any;
    if (any) seen[token] = { kind: "inset", value: pl || pt };
  }

  function tag(layer, x, y, text, kind) {
    var el = document.createElement("span");
    el.className = "pd-rl-tag";
    el.dataset.kind = kind;
    el.textContent = text;
    layer.appendChild(el);
    var max = layer.clientWidth - el.offsetWidth;
    el.style.left = Math.max(0, Math.min(max, x)) + "px";
    el.style.top = Math.max(0, y) + "px";
  }

  function highlight(root, test) {
    var layer = layerOf(root);
    layer.querySelectorAll(".pd-rl-tag").forEach(function (el) {
      el.remove();
    });
    var hot = [];
    layer.querySelectorAll(".pd-rl").forEach(function (el) {
      var on = !!test && test(el);
      el.classList.toggle("pd-hot", on);
      if (on) hot.push(el);
    });
    root.classList.toggle("pd-focus", !!test);
    var named = {};
    hot.forEach(function (el) {
      var key = el.dataset.owner + el.dataset.kind;
      if (named[key]) return;
      named[key] = true;
      var x = parseFloat(el.style.left);
      var y = parseFloat(el.style.top);
      var w = parseFloat(el.style.width);
      var h = parseFloat(el.style.height);
      var text = el.dataset.token + " " + fmt(Math.min(w, h));
      if (el.dataset.kind === "gap" && h > w) tag(layer, x + w / 2 - 20, y - 18, text, "gap");
      else if (el.dataset.kind === "gap") tag(layer, x + 4, y + h / 2 - 7, text, "gap");
      else tag(layer, x + 2, y - 16, text, "inset");
    });
  }

  function chips(root, seen) {
    var host = root.querySelector("[data-redline-tokens]");
    if (!host) return;
    host.textContent = "";
    Object.keys(seen)
      .sort(function (a, b) {
        var ka = seen[a].kind === "inset" ? 0 : 1;
        var kb = seen[b].kind === "inset" ? 0 : 1;
        return ka - kb || seen[a].value - seen[b].value;
      })
      .forEach(function (token) {
        var chip = document.createElement("button");
        chip.type = "button";
        chip.className = "pd-token";
        chip.dataset.kind = seen[token].kind;
        chip.textContent = token + " " + fmt(seen[token].value);
        function on() {
          highlight(root, function (el) {
            return el.dataset.token === token;
          });
        }
        function off() {
          highlight(root, null);
        }
        chip.addEventListener("mouseenter", on);
        chip.addEventListener("focus", on);
        chip.addEventListener("mouseleave", off);
        chip.addEventListener("blur", off);
        host.appendChild(chip);
      });
  }

  function draw(root) {
    var layer = layerOf(root);
    layer.textContent = "";
    var seen = {};
    if (root.classList.contains("pd-redline-on")) {
      var origin = root.getBoundingClientRect();
      root.querySelectorAll("[data-inset], [data-gap]").forEach(function (el, i) {
        el.dataset.pdOwner = String(i);
        if (el.hasAttribute("data-inset")) inset(layer, origin, el, String(i), seen);
        if (el.hasAttribute("data-gap")) gaps(layer, origin, el, String(i), seen);
      });
    }
    chips(root, seen);
    root.classList.remove("pd-focus");
  }

  function bind(root) {
    var toggle = root.querySelector("[data-redline-toggle]");
    var frame = 0;
    function later() {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(function () {
        draw(root);
      });
    }
    function sync() {
      root.classList.toggle("pd-redline-on", !toggle || toggle.checked);
      later();
    }
    root.addEventListener("mouseover", function (event) {
      if (!root.classList.contains("pd-redline-on")) return;
      if (event.target.closest(".pd-tools")) return;
      var layer = layerOf(root);
      var owner = event.target.closest("[data-pd-owner]");
      while (owner && root.contains(owner) && !layer.querySelector('.pd-rl[data-owner="' + owner.dataset.pdOwner + '"]')) {
        owner = owner.parentElement && owner.parentElement.closest("[data-pd-owner]");
      }
      if (!owner || !root.contains(owner)) return highlight(root, null);
      var id = owner.dataset.pdOwner;
      highlight(root, function (band) {
        return band.dataset.owner === id;
      });
    });
    root.addEventListener("mouseleave", function () {
      highlight(root, null);
    });
    if (toggle) toggle.addEventListener("change", sync);
    if (window.ResizeObserver) new ResizeObserver(later).observe(root);
    if (document.fonts && document.fonts.ready) document.fonts.ready.then(sync);
    sync();
    return later;
  }

  function init() {
    document.querySelectorAll("[data-redline]").forEach(function (root) {
      root.pdRedraw = bind(root);
    });
  }

  window.pennaDesign = { draw: draw, bind: bind };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init);
  else init();
})();
