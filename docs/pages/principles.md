# Principles

penna draws zz and nidus: tools people keep open all day, mostly from the keyboard, where the work matters more than the interface around it. Six principles follow from that. Every rule, scale and component in penna keeps one of them, and when something is not covered, they decide it.

## The keyboard comes first

Every action has a keyboard path, and the interface shows it. Shortcuts sit beside the menu rows that run them, and a ring appears whenever the keyboard moves focus. The pointer and touch reach the same actions, sized for them.

```figure story=menus/popup-menu
```

## Calm, dense screens

Most text on a screen is one size. Weight and a few larger steps carry the hierarchy, and color is kept for meaning. Spacing follows how things relate, so even a dense screen groups clearly. Density is a setting people choose, never a squeeze.

```figure uses=label,kbd
<div class="docs-full penna-stack" data-gap="section" style="max-width: 460px">
  <div class="penna-stack" data-gap="tight">
    <span class="penna-type" data-type="title" data-penna-text>Keyboard</span>
    <span class="penna-type docs-text" data-type="body" data-muted data-penna-text>Shortcuts work in every window, including the storybook.</span>
  </div>
  <div class="penna-settings" style="padding: 0">
    <div>
      <h2>Panes</h2>
      <div data-penna="settings.card">
        <div class="penna-setting"><div class="penna-copy">Next pane<small>Move to the pane on the right.</small></div><span class="penna-inline" data-gap="tight"><kbd class="penna-kbd" data-penna="keycap" data-kbd="ctrl-b"></kbd><kbd class="penna-kbd" data-penna="keycap" data-kbd="right"></kbd></span></div>
        <div class="penna-setting"><div class="penna-copy">Previous pane<small>Move to the pane on the left.</small></div><span class="penna-inline" data-gap="tight"><kbd class="penna-kbd" data-penna="keycap" data-kbd="ctrl-b"></kbd><kbd class="penna-kbd" data-penna="keycap" data-kbd="left"></kbd></span></div>
      </div>
    </div>
  </div>
</div>
```

## One highlight

One color marks what is on, picked or in focus. In the default look that color is the ink itself, so the interface stays black and white until someone picks an accent. Red, amber and green mean something went wrong, needs a look or went right, and always come with an icon or a word. A view has one main action at most.

```figure
<div class="docs-compare docs-full">
  <div>
    <div class="docs-actions">
      <button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Cancel</span></button>
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Add host</span></button>
    </div>
    <div class="docs-verdict" data-verdict="do"><i data-icon="circle-check"></i><span class="penna-type" data-type="small" data-penna-text>One main action, drawn in the highlight.</span></div>
  </div>
  <div>
    <div class="docs-actions">
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Cancel</span></button>
      <button class="penna-button" data-penna="button" data-tone="success"><span class="penna-label" data-penna-text>Add host</span></button>
    </div>
    <div class="docs-verdict" data-verdict="dont"><i data-icon="circle-x"></i><span class="penna-type" data-type="small" data-penna-text>Two loud buttons, and green that means nothing.</span></div>
  </div>
</div>
```

## The work leads, chrome recedes

Bars, headers and toolbars use quiet controls with no fill until they are pointed at, muted labels and the smaller sizes, so the eye goes to the content. Text people read, like documents and chat, is larger and more open than text people scan.

```figure story=title_bar/bar
```

## Every state shows

A control always tells what it is doing: resting, pointed at, pressed, focused, picked, switched off, busy or in error. Feedback comes the moment something is touched. Motion is short, shows where something came from or went, and turns off for people who ask for less motion.

```figure story=buttons/states
```

## One kit everywhere

The same components draw native windows on every desktop, the phone and this site, in three looks, light and dark. A look changes materials, corners and typefaces, never sizes, spacing or behavior. Every picture on these pages is penna itself: switch the look in the corner, or use the switch on an example to see it drawn natively.

```figure story=selection/switch
```
