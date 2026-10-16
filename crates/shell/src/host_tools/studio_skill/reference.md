# Splash for studio apps

What a studio app's `main.splash` may use. A studio app is an offline,
storage-only script app: it runs in a policed isolate with no network, no
host services and no web resources. Everything below is taken from the
runtime and from the working system apps; if something is not on this page,
it does not exist for your app. Do not invent widgets, methods or modifiers.

## Shape of a program

```splash
let notes = []                       // state: top-level let, persists for the session

fn load(){                           // functions may use ui.* (they run after the body)
    if fs.exists("notes.json") {
        let v = fs.read("notes.json").parse_json()
        if v != nil { notes = v }
    }
    ui.list.render()
}

start_timeout(0.05, || load())       // ui is injected AFTER the body evaluates

SolidView{width: Fill height: Fill flow: Down padding: 16 draw_bg.color: #xffffff
    list := ScrollYView{width: Fill height: Fill flow: Down on_render: || {
        if notes.len() == 0 { Label{text: "No notes yet." draw_text.color: #x8e8e93} }
        for i in notes.len() { Label{text: notes[i] draw_text.color: #x1c1c1e} }
    }}
}
```

- Declarations (`let`, `fn`) first, then exactly one root widget. The host
  prepends the widget prelude and `fs` itself and wraps your body in a view
  that provides the width: the root uses `width: Fill`, never a pixel width.
- `ui` is injected after the body is evaluated. It works inside every `fn`
  and handler, not in top-level statements during the first evaluation.
  Start work with `start_timeout(0.05, || load())`, as every system app does.
- `name := Widget{…}` makes a widget addressable as `ui.name`. A `:=` id is
  a field of its direct parent only: in `Card{ inner := View{ b := Label{} } }`
  the label is `Card{inner.b.text: "x"}`, and `Card{b.text: …}` fails with
  "field b not found". Name every wrapper you need to reach through. Do not
  declare `ui` yourself.

## Syntax essentials

- Values: numbers (`0`, `2.5`), strings in double quotes, `true`, `false`,
  `nil`, arrays `[1 2 3]`, objects `{text: "a" done: false}`, colors
  `#x1c1c1e`. Items in arrays, argument lists and object bodies are
  separated by whitespace, newlines or commas; all three work.
- `let x = 1` declares, `x = 2` assigns; `+= -= *= /=` work on numbers,
  `+=` also merges objects: `items[i] += {done: true}`.
- `fn add(text){ … }` declares a function; `return` leaves it. Closures are
  `|| expr`, `|x, y| expr` or `|text| { … }`. Statements inside `{ … }` are
  separated by newlines or `;`.
- `if a { } elif b { } else { }`; `for i in n` (0 to n-1), `for v in array`,
  `for i v in array`, `for k v in object`, `a..b` ranges, `while`, `loop {}`,
  `break`, `continue`. There is no `range()`.
- Operators: `+ - * / %`, `== != < <= > >=`, `&&`, `||`, `!`. Numbers become
  text with `"" + n`. `"a" + 1` is `a1`.
- Two object literals side by side are two items, never one extended object:
  to reuse a shape bind it once (`let Row = View{…}`) and instantiate it
  (`Row{…}`), or merge with `+=`.
- Missing names and fields are errors, not nil
  (`variable no_such_thing not found in scope`, `property missing_field not
  found in prototype chain`). Guard optional fields on data you control.
- Reserved words cannot be variable names: `me scope self nil true false ok
  let var mut fn if elif else for in while loop match return break continue
  and or is do try use`. A top-level `let` or `fn` that reuses a prelude name
  (`tick`, `floor`, a widget name) opens a child scope instead of replacing
  it, and `fn tick(` starts an implicit one-second timer: pick other names.
- `as int` casting produces NaN: keep numbers as numbers and format with
  `"" + n`, `floor(n)` or `round(n)`.

## `ui.<id>`: reading and changing widgets

Lookup searches the handler's subtree, then the whole app; `ui.root` is the
root. A miss is the error `widget '<id>' not found in tree`; an unknown
method is `widget method <m> not found`.

| Call | On | Notes |
| --- | --- | --- |
| `ui.x.text()` / `ui.x.set_text(s)` | Label, LinkLabel, TextInput, buttons | `set_text` wants a string: `set_text("" + n)` |
| `ui.x.set_visible(b)` / `ui.x.visible()` | any widget | views take a bool only |
| `ui.x.render()` | View, SolidView, RoundedView, ScrollYView, ScrollXView | re-runs the view's `on_render` |
| `ui.x.value()` / `ui.x.set_value(n)` | Slider | no script callback |
| `ui.x.checked()` | CheckBox | no setter |
| `ui.x.on_click()` | Button family | fires the handler |
| `open() close() toggle() is_open()` | SlidePanel | |

## Events

| Handler | Widget | Arguments |
| --- | --- | --- |
| `on_click: \|\| …` | Button, ButtonFlat, ButtonFlatter | none |
| `on_click: \|active\| …` | CheckBox | `active` |
| `on_change: \|text\| …` | TextInput | the new text (same as `ui.<id>.text()`) |
| `on_return: \|text\| …` | TextInput | the text when Return is pressed |
| `on_tap: \|x, y\| …` | GestureView | point in the view |
| `on_double_tap: \|x, y\|`, `on_long_press: \|x, y\|` | GestureView | |
| `on_swipe: \|dx, dy\|` | GestureView | one axis is 0 |
| `on_pan: \|dx, dy, phase\|` | GestureView | phase 0 begin, 1 move, 2 end |
| `on_pinch: \|scale, x, y, phase\|` | GestureView | |
| `on_detent: \|d\|` | SheetView | 0 peek, 1 half, 2 full |
| `on_render: \|\| { children }` | View family | none; the closure's widgets become the children |

GestureView thresholds: tap slop 10 points, swipe at least 60 points within
0.45 s, double tap within 0.3 s.

## Timers, time and math

Bare names: `start_timeout(secs, fn)` and `start_interval(secs, fn)` return a
timer id for `stop_timer(id)`; `time_now()` is Unix seconds as a float;
`local_time(epoch?)` gives `{year month day hour minute second weekday}`
(weekday 0 is Sunday); `random()` is 0..1, `random_u32()` an integer.

```splash
let timer = nil
fn start(){ timer = start_interval(0.5, || step()) }
fn stop(){ stop_timer(timer) }
```

A body containing `fn tick(` gets an implicit 1 Hz interval calling `tick`;
name your function something else unless you want that. Math is bare too:
`floor ceil round abs min max clamp pow sqrt sin cos …` (`floor(2.7)` is 2).

## Storage: `fs.*`

The app's own jail. Paths are relative to it; a leading `/` means the jail
root; `..` above the root is an error; links are refused.

| Call | Returns |
| --- | --- |
| `fs.read(path)` | string |
| `fs.read_bytes(path)` | byte array |
| `fs.write(path, text)` | nil |
| `fs.append(path, text)` | nil |
| `fs.exists(path)` | bool |
| `fs.list(dir)` | sorted names, directories end in `/` |
| `fs.mkdir(path)` | nil, recursive |
| `fs.remove(path)` | nil; a file or an empty directory |

No rename, stat or binary write. A studio app's whole jail is at most 1 MiB,
with at most 256 entries, 16 levels and 128-character names. A failure
returns an error value (not nil): `file not found`, `path escapes the app's
storage`, `file too large`, `app storage is full`, `too many files`, `data
must be a string`, `not a directory`. Declare `"capabilities": ["storage"]`
in the manifest when you store anything. Keep state as one JSON file:
`fs.write("state.json", state.to_json())` and
`fs.read("state.json").parse_json()`.

## Data and strings

| | Available | Notes |
| --- | --- | --- |
| JSON | `s.parse_json()`, `v.to_json()` | `parse_json` never raises, and invalid JSON does not reliably give nil: check the fields you need. `to_json` stops at depth 24. |
| Strings | `len trim split(p) search(p) replace(p, r) strip_prefix strip_suffix url_encode url_decode to_f64 match_str match_all` | `to_f64` gives NaN (not nil) for `""`, non-numbers and untrimmed text; NaN compares false with everything, so guard with `if v >= 0`. `search` gives an index or -1. `replace` changes the first occurrence only; replace all with `regex("-", "g")`. |
| Regex | `regex(pattern, flags)` with `.test(s)`, `.exec(s)` | case-insensitive contains: `regex("word", "i").test(s)` |
| Missing | `to_lower to_upper contains starts_with ends_with` | use `search(p) == 0`, `search(p) >= 0`, or a regex |
| Arrays | `push pop clear len remove(i) retain(fn)` | no `sort map filter join insert index_of slice`; copy with a loop |
| Objects | `o.k`, `o[k]`, `delete`, `extend`, `o += {k: v}` | |
| Numbers to text | `"" + n` | |

## Widgets

Every widget below resolves in the isolate.

| Widget | Use |
| --- | --- |
| `View`, `SolidView`, `RoundedView` | containers; `SolidView` or `RoundedView` for a filled background |
| `ScrollYView`, `ScrollXView`, `ScrollXYView` | scrolling containers, usually with `on_render` |
| `Label`, `LinkLabel` | text; the default color is white, so set `draw_text.color` |
| `TextInput` | text entry; needs a numeric `height`; `empty_text:` is the placeholder |
| `Button`, `ButtonFlat`, `ButtonFlatter` | buttons with `text:` and `on_click`; they hold no children |
| `Html`, `Markdown` | rich text |
| `GestureView` | taps, swipes, pinches around any content; the way to make a rich row tappable |
| `SheetView` | bottom sheet with detents |
| `Hr`, `Vr`, `Icon`, `LoadingSpinner`, `Slider`, `CheckBox`, `CheckBoxFlat`, `Toggle`, `ToggleFlat`, `RadioButton`, `RadioButtonFlat`, `DropDown`, `PopupMenu`, `ScrollBar`, `ScrollBars` | controls |
| `Image` | only from bytes in the jail: `ui.x.load_image_from_data_async(fs.read_bytes("pic.png"))`; never from a URL |

Not available (they fail silently): `Stack`, `Divider`, `ProgressBar`,
`IconButton`, `ToggleButton`, `ListView`, `Grid`, `ColorPicker`,
`ScrollPair`. Use `Hr{height: 1 width: Fill}` for a divider,
`Slider{value: 0.65 is_read_only: true}` for a progress bar, and a row of
`ButtonFlat`s for tabs (`TabBar` renders at zero size).

## Layout and styling

- Common properties: `width`/`height` (`Fill`, `Fit` or a number),
  `flow: Down | Right`, `spacing: n`, `padding: n` or
  `padding: Inset{top: 8 bottom: 8 left: 12 right: 12}`,
  `align: Align{x: 0.5 y: 0.5}`, `margin`.
- The root is `width: Fill height: Fill`. Inside a parent with a real
  height, `height: Fill` is fine; inside a `Fit` parent a `Fill` height
  collapses to zero, so give a `ScrollYView` a parent with a real height or
  a numeric `height`. Rows of controls are `height: Fit`.
- Override syntax: `child.property: value`, where every segment of the path
  is a `:=` id. `draw_bg.color: #xffffff` and
  `draw_text.text_style.font_size: 15` set one nested field;
  `draw_bg +: {color: … color_hover: … color_down: …}` sets several.
  `draw_bg.border_radius` takes a float (`12.0`), not an Inset.
- Hex colors: always write `#x` before the digits (`#x1c1c1e`, `#x2ecc71`).
  Without it a hex value with an `e` next to digits is read as an exponent.
- Text is white by default. Set `draw_text.color` on every label and button
  that sits on a light background. Bold or larger text:
  `draw_text.text_style: theme.font_bold{font_size: 28}` or
  `draw_text.text_style.font_size: 15`.
- Filled panels: `SolidView{draw_bg.color: …}` (the template's root) or
  `RoundedView{draw_bg.color: … draw_bg.border_radius: 12.0}`. If text
  inside a filled container does not show, add `new_batch: true` to that
  container.
- A `ButtonFlat` with only `text:` draws white text on a light outline,
  invisible on a white app. Style it as the template does, both `draw_bg +:`
  and `draw_text +:`. To reuse a style, bind it once and instantiate it:
  `let Chip = ButtonFlat{height: 40 draw_bg +: {…} draw_text +: {…}}`, then
  `Chip{text: "10%" on_click: || set_tip(10)}`.
- `ButtonFlat` holds no children: a `Label` inside it is not drawn and the
  button's own text disappears. Use `text:`, or `GestureView{on_tap: |x, y| …}`
  around the content for a rich tappable row.
- A page drawn by a plain `View` shows the host's light or dark background.
  Fixed dark text on it vanishes in dark mode: paint your own surface with
  `SolidView` when you fix text colors, and check both appearances.

## State and re-rendering

- Top-level `let` variables persist across handlers for the session:
  `let count = 0` then `count += 1` in `on_click` counts 1, 2, 3.
- A `for` loop in the body draws its widgets once, at build time; changing
  the array later draws nothing new. For a list that changes, keep the
  data in an array, draw it inside `ScrollYView{on_render: || { … }}`, and
  call `ui.<list>.render()` after every change. Write the empty state as a
  separate `if list.len() == 0 { … }` followed by the `for`, not as
  `if … else for …`, which left stale rows on screen.
- Values that are only displayed: `ui.display.set_text("" + total)`.
- The visual `checked` state of `CheckBox`, `RadioButton` and `ToggleFlat`
  does not persist across handlers. For a durable boolean use a `let` and a
  `ButtonFlat` whose `on_click` flips it and updates a label.

```splash
let items = [{text: "Task 1" done: false} {text: "Task 2" done: false}]

fn toggle_item(i){
    if i >= items.len() { return }
    items[i] += {done: !items[i].done}
    ui.list.render()
}
fn clear_done(){
    items.retain(|it| !it.done)
    ui.list.render()
}
```

## Not available in a studio app

- The network: no `net` module, `net.http_request`, sockets or servers.
- Host services: `host.request(...)` is refused for every service; there is
  no `host.prompt`. Do not call `host.*` or `sys.*`.
- Web resources: no `http_resource(...)`, no `Image{src: …}` from a URL, no
  `{{assets}}` route, no `WebReader`. The bundle check refuses a
  `main.splash` that contains `://`, `http_resource(` or `{{assets}}`.
- Devices: no `CameraPreview`, `MapView`, location or microphone.
- Accounts, an app agent, `storage.accounts`, WebAssembly components and
  signatures: the manifest may declare only `storage`.
- Password, PIN and one-time-code fields: `is_password: true` or a password
  `content_type` renders an inert field and the bundle is refused.

## Limits

| Limit | Value | What you see |
| --- | --- | --- |
| One handler, timer, callback or the body | 200 000 instructions and 64 ms | `script instruction limit exceeded` or `script time budget exceeded`; the app keeps running |
| Instructions per session | at most 5 000 000 in a studio app | the app is stopped: `its instruction budget is spent` |
| Heap | at most 16 MiB | `script heap allocation limit exceeded while <op>` |
| Storage | at most 1 MiB, 256 entries | `app storage is full`, `file too large`, `too many files` |
| Source | `main.splash` at most 64 KiB | refused by the bundle check |

Keep handlers short: no endless loops, no work on every frame, no large
string building in `on_render`.

## When something is wrong

- A body that fails to evaluate shows an empty app. Re-read the syntax
  rules above: a comma-free property list, a `#x` color, a missing `}` or
  a modifier chain (`.padding(8)`) are the usual causes.
- `studio.inspect` reports layout findings on your own instance: empty
  geometry (a widget with no size, often a `Fill` inside a `Fit`), clipped
  text and buttons too small to tap. Fix them before installing.
- A handler that does nothing usually names a widget that is not a `:=` id
  of a reachable parent, or a method that does not exist for that widget.
  Give the widget an id, check the method table above, and drive the
  handler again with `studio.input`.
- Runtime errors in handlers are logged by the host, not returned to you:
  test each control by sending its event and inspecting the result.
