# Button

A button starts an action now. Pick a kind, give it a label or an icon, and handle the click.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>Two things don't match the spec yet: the caret stays one size on every button instead of matching a select's chevron, and warning and success buttons are still offered, as the first picture shows.</span></div></div></div>
```

To go somewhere, use a link. Inside a menu, use a menu row. For a setting that takes effect at once, use a switch, and to pick one of a few views, a segmented control.

## Kinds

```rust example story=buttons/variants
Button::new("primary").label("Primary").primary()
Button::new("secondary").label("Secondary")
Button::new("ghost").label("Ghost").ghost()
Button::new("danger").label("Danger").danger()
Button::new("link").label("Link").link()
```

- **Primary** is the main action of a view, drawn in the highlight. A dialog, a form or a pane has one at most.
- **Secondary**, the default, is every other action.
- **Ghost** has no fill until it is pointed at. It belongs in bars, toolbars and rows, where a fill on every button would be noise.
- **Outline** is a secondary action on a ground with its own fill, where a soft fill would disappear.
- **Danger** is for an action that destroys something or can't be undone.
- **Link** goes somewhere: a page, an address, a pane of settings. **Text** is a plain action inside a sentence.

Warning and success colors mean status, so a button never takes them on its own. A notice's action is a primary button on the notice, which draws it in the notice's color.

```figure story=status/notices
```

## Anatomy

```figure
<div class="docs-row" style="gap: 32px; flex-wrap: wrap">
  <div class="penna-stack" data-gap="related"><span class="penna-type" data-type="small" data-muted data-penna-text>Icon and label</span><button class="penna-button" data-penna="button" data-size="xlarge"><i data-icon="plus"></i><span class="penna-label" data-penna-text>Add host</span></button></div>
  <div class="penna-stack" data-gap="related"><span class="penna-type" data-type="small" data-muted data-penna-text>Caret</span><button class="penna-button" data-penna="button" data-size="xlarge" data-caret><span class="penna-label" data-penna-text>Model</span><i class="penna-caret" data-icon="chevron-down"></i></button></div>
  <div class="penna-stack" data-gap="related"><span class="penna-type" data-type="small" data-muted data-penna-text>Icon only</span><button class="penna-button" data-penna="button.quiet" data-size="xlarge" data-tooltip="Notifications" aria-label="Notifications"><i data-icon="bell"></i></button></div>
</div>
```

A button has a container and up to three parts inside it, in this order:

- **Container**: the shape, with its fill, outline and corners. The kind and the look draw it, and the size sets its height and the room at its sides.
- **Icon**: optional, before the label. While the button is busy it turns into a spinner.
- **Label**: the text, sized by the button's size.
- **Caret**: optional, a small chevron at the end, for a button that opens a menu. It matches a select's chevron at the same size.

A button with only an icon is square, and always has a tooltip that names its action. When the keyboard reaches a button, a ring appears just outside the container.

## Sizes

Medium is the default and follows the density setting. Small and extra small fit toolbars, lists and rows that repeat; large and extra large give the one main action of a sparse screen room; touch is sized for a finger. Every button in a row has the same size.

```rust example story=buttons/sizes
Button::new("xsmall").label("Extra small").xsmall()
Button::new("small").label("Small").small()
Button::new("medium").label("Medium")
Button::new("large").label("Large").large()
Button::new("xlarge").label("Extra large").xlarge()
Button::new("touch").label("Touch").touch()
Button::new("bell").icon(IconName::Bell).tooltip("Notifications")
Button::compact_icon("more", IconName::Ellipsis).tooltip("More")
```

## States

```rust example story=buttons/states
Button::new("save").label("Save").disabled(true)
Button::new("tab").label("Ghost").ghost().selected(true)
Button::new("saving").label("Saving").loading(true)
```

A disabled button leaves the keyboard's path but still shows its tooltip. A busy button keeps its label and width and ignores clicks. A picked button stands for something that is on, in a group of them.

Pointing at a button lifts it. Each picture below shows the state on the left, beside a resting copy.

```figure story=buttons/hovered
```

Pressing deepens the fill for as long as the button is held. Under a finger, a press highlight takes the place of both.

```figure story=buttons/pressed
```

Keyboard focus draws a ring outside the edge. Only the keyboard brings it up; a click does not.

```figure story=buttons/focused
```

## Labels

A label is a verb or a short phrase that starts with one, in sentence case, with no closing period. It names the action, so the result is clear before the click. A label ends in an ellipsis when the action asks for more first, such as opening a dialog.

```figure
<div class="docs-compare docs-full">
  <div>
    <div class="docs-actions">
      <button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Keep branch</span></button>
      <button class="penna-button" data-penna="button" data-tone="danger"><span class="penna-label" data-penna-text>Delete branch</span></button>
    </div>
    <div class="docs-verdict" data-verdict="do"><i data-icon="circle-check"></i><span class="penna-type" data-type="small" data-penna-text>Each label says what happens.</span></div>
  </div>
  <div>
    <div class="docs-actions">
      <button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>No</span></button>
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>OK</span></button>
    </div>
    <div class="docs-verdict" data-verdict="dont"><i data-icon="circle-x"></i><span class="penna-type" data-type="small" data-penna-text>OK and No make people read the question again.</span></div>
  </div>
</div>
```

## Placement

Buttons in a group sit close together. A dialog's or a form's actions sit at its end, with the main action last and its cancel just before it. A dialog that destroys something confirms with a danger button, never a primary one.

```figure
<div class="docs-compare docs-full">
  <div>
    <div class="docs-actions">
      <button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Cancel</span></button>
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Save</span></button>
    </div>
    <div class="docs-verdict" data-verdict="do"><i data-icon="circle-check"></i><span class="penna-type" data-type="small" data-penna-text>The main action last, at the end.</span></div>
  </div>
  <div>
    <div class="docs-actions" style="justify-content: flex-start">
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Save</span></button>
      <button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Cancel</span></button>
    </div>
    <div class="docs-verdict" data-verdict="dont"><i data-icon="circle-x"></i><span class="penna-type" data-type="small" data-penna-text>The main action first, at the start.</span></div>
  </div>
</div>
```

## API

| Rust | HTML | What it does |
| --- | --- | --- |
| `.label(text)` | `.penna-label` | Text inside the button |
| `.icon(name)` | `<i data-icon>` | An icon before the label |
| `.primary()`, `.danger()`, `.ghost()`, `.link()`, `.text()` | `data-tone`, `data-prominent`, `data-penna="button.quiet"`, `data-variant` | The kind |
| `.emphasis(Emphasis::Outline)` | `data-penna="button.outline"` | The outline kind |
| `.xsmall()`, `.small()`, `.large()`, `.xlarge()`, `.touch()` | `data-size="xsmall"`, `"small"`, `"large"`, `"xlarge"`, `"touch"` | A step on the size ladder; medium is the default |
| `.selected(true)` | `data-state="current"` | Draws it picked |
| `.disabled(true)` | `disabled` | Switches it off |
| `.loading(true)` | `data-loading` | Shows it busy |
| `.dropdown_caret(true)` | `data-caret` and a `.penna-caret` | A caret for a button that opens a menu |
| `.tooltip(text)` | `data-tooltip` | A tooltip on hover; required with only an icon |
| `.on_click(handler)` | a `click` listener | Runs on release |
