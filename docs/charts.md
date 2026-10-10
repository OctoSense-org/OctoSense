# Native charts in OctoScript apps

English | [简体中文](charts.zh-CN.md)

The shell registers the pinned Makepad D3 widgets in ordinary Splash isolates.
An installed app and its Glance card use the same chart vocabulary, while
retaining their own app identity, storage, consent and lifecycle. Charts draw
with Makepad's native vector renderer; they do not require a browser or JavaScript.

This integration is newer than desktop RC4. RC4 does **not** register `d3`.
Apps that require charts can declare `"host_api":{"required":{"charts.d3":1}}`
alongside `"requires":["host-api-v1"]`. This is a compatibility check, not a
permission. `runtime.describe` reports `charts.d3` as a non-callable runtime ABI.
An app with a useful non-chart presentation may omit the requirement and guard
its chart branch with `try { d3.BarChart != nil } catch { false }`.

The full chart names are `d3.LineChart`, `d3.BarChart` and `d3.Heatmap`:

| Widget | Data | Script methods | Events |
| --- | --- | --- | --- |
| `LineChart` | Numbers (x is the index), `[x y]` pairs, or `{x: … y: …}` objects | `set_data`, `set_domain(y_min, y_max)`, `data` (returns y values) | `on_click`, `on_hover`: nearest x index |
| `BarChart` | Numbers, with a separate string `labels` array | `set_data`, `set_labels`, `set_domain`, `data` | `on_click`, `on_hover`: bar index |
| `Heatmap` | Array of numeric rows | `set_data` | `on_click`, `on_hover`: row and column |

```text
line := d3.LineChart {
    width: Fill height: 240
    data: [[0 3], [1 7], [2 5]]
    on_click: |index| ui.status.set_text("Selected point " + index)
}
```

Adjacent nested arrays need commas. `Heatmap` supports `colormap` values
`viridis`, `plasma`, `inferno`, `magma`, `coolwarm`, `turbo` and `gray`, plus
`cell_gap`. Its colors normalize to the current grid's minimum and maximum;
it has no fixed color-domain or built-in date labels. Line x axes are numeric,
not formatted dates. These wrappers do not expose multi-series, zoom or brush
APIs. Supply surrounding labels and selection details in ordinary widgets.

Empty data renders the library's demonstration values. Never use an empty
chart as a loading, missing-data or failed-request state: show an explicit
message and instantiate the chart only when genuine data is available.

The shell also registers the other native D3 chart families at the same pin.
It intentionally excludes `d3.Octoscript`, the library's older nested-VM host;
apps use the normal host-managed Splash lifecycle. The crate README's historic
axis-label warning refers to an older Makepad development revision, not a
current acceptance result. Validate the actual chart's labels and mark hit
targets on each shipped platform.

The native fixture is `crates/shell/examples/charts-host.rs`; its driver is
`tools/test-native-charts.py`. It checks chart areas, clicks, data replacement
and script errors, and saves frames for visual inspection. It waits three
seconds before the initial capture because the widget snapshot can precede
native font and vector uploads. `--script=<path>` on the fixture host accepts a
local Splash body for UI diagnosis; this example registers no account or host
service broker.

The macOS fixture passed eight checks, including all three chart click handlers
and replacing/readback of line and bar data. The initial and updated frames
were inspected: axes, numeric ticks, category labels and heatmap cells are
visible. The Glance unit regression uses the real workspace renderer; it and
the ordinary-isolate and cold compatibility regressions passed. This is
development-host evidence, not acceptance of a released chart-enabled shell.
Android, Windows and Linux native chart rendering remain **unverified**.
