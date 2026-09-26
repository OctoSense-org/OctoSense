# WeatherIcon

A live, shader-animated weather glyph. Set its condition with `draw_bg.cond`:

```
WeatherIcon{ draw_bg.cond: <N> width: 60 height: 60 }
```

It MUST be `draw_bg.cond` — a bare `cond:` is silently ignored.

## Condition map (`draw_bg.cond`)

| N | Condition |
|---|-----------|
| 0 | clear / sunny |
| 1 | partly cloudy |
| 2 | cloudy / overcast |
| 3 | rain / drizzle |
| 4 | thunderstorm |
| 5 | snow |
| 6 | wind |
| 7 | fog / haze / mist |

## Animation

It is a live shader-animated glyph: rays rotate, rain/snow fall, wind/fog drift,
lightning flashes. The animation pump auto-starts whenever a card contains a
`WeatherIcon` — no extra setup is required.
