# Grok Voice design language (v2)

Read this before changing any screen, sheet, widget, or theme. It is the contract for agents. Themes change tokens. They do not invent new colors, radii, or shadows in a component.

The app is a phone instrument: Talk, Notes, Calendar, Time, Finance, Settings. Later phases fold Calendar and Time into Agenda and move People under Settings. It should feel like a quiet luxury object. The references are the My Porsche app and Chanel’s digital surfaces, not a SaaS dashboard and not a magazine template.

v2 allows cards, a larger radius scale, hero figures, and charts. It keeps the restrained dark posture, one metal accent, and the bans on glass, gradients, and emoji.

## Luxury reference

**Porsche (My Porsche).** Near-black or warm-gray fields, never pure black. Type does the work: a tightly tracked sans for titles, small labels only where a group needs a name. Icons are thin. One accent exists and is used as a precise mark (a selected tab, a focus line, a status), not as a frame around every control. Primary actions are solid fills. Secondary actions are text.

**Chanel.** Editorial restraint. Generous empty space. The name and the section title are the loudest things on the screen. Labels, when they appear, are small, widely tracked, and rare. Links and secondary actions are underlined type, not pills. Nothing is decorated with corner ornaments, gradients, or glass.

What we take from both: typography-led hierarchy, tonal surfaces, one metal accent, solid or text controls, and empty space as a material. v2 adds a figure-first hierarchy: the key number, the orb, or the chart leads. Supporting text stays to one sentence.

## Floors that do not move

- No pure black (`#000`) and no pure white (`#FFF`) in surfaces or text. Near-black and near-white only.
- Minimum text size is 13px (`--type-caption`). That includes navigation labels, captions, and calendar counts. Type is rem-based so Android `textZoom` scales it with the system font size.
- Touch targets are at least 48px (`--touch`). A thin icon still sits in a 48px hit area. Hit areas do not overlap.
- Text on the canvas meets WCAG AAA where the existing palette tests require it (7:1 for primary text, 4.5:1 for muted text). Borders that carry meaning meet 3:1. `--warning` meets 3:1 on `--surface-0`. Sand `--brand` meets 4.5:1 on `--surface-0` because it can appear as text. Do not communicate state by color alone.
- Themes are Graphite, Ink, Stone, Midnight, Carbon, Ivory, Sand, Forest, Plum, System, and Custom. One `data-appearance` attribute on `<html>` swaps the theme. `data-scheme` is `dark` or `light`. Custom paints the same semantic tokens from script.
- Dark themes are the default posture (Graphite). Light themes are Stone, Ivory, and Sand.

## Radius

The scale is restrained. There are no pills on buttons.

| Token | Size | Use |
| --- | --- | --- |
| `--radius-xs` | 4px | Inputs, chips |
| `--radius-sm` | 8px | Buttons, segmented controls |
| `--radius-md` | 12px | Cards |
| `--radius-lg` | 20px | Sheet top corners. A FAB uses 16px |
| `--radius-full` | 9999px | Orb, avatars, the nav pill, status dots |

`--radius` (2px) remains for screens that have not moved to these tokens yet. New and restyled surfaces use the scale above. Circles stay circles.

## Cards, figures, and charts

Cards are allowed for an object or a figure group: `--surface-1`, `--radius-md`, no stroke in dark themes, `--e1` in light themes. A card is a tappable object or a group of figures. Settings stay a list.

A list of settings does not each get a card. Nested cards are a defect. If a surface sits on a surface, remove the inner one.

Hero figures use a 2:1 number-to-unit ratio (`--type-display` / `--type-unit`), baseline-aligned, number first, tabular numerals. Do not set the currency and the amount in one identical size. Never hardcode a currency.

Charts read the semantic tokens and the `--series-*` aliases. They do not invent a second palette.

## Borders and elevation

A full outline around a region is not the default. Separate content with space, a tonal step (`--surface-0` to `--surface-1` to `--surface-2`), or a hairline (`--line-soft`) on one edge. A real border (`--line`, 1px) is for a text field’s baseline, a destructive confirmation, or keyboard focus.

Do not draw decorative corner marks on panels, charts, or cards.

- Dark themes: `--e1` and `--e2` are none. Depth is `--surface-1`, `--surface-2`, `--surface-3`, and a hairline.
- Light themes: `--e1` is `0 1px 2px` at 4% black. `--e2` is the sheet and FAB shadow. No hard offset shadows, no glow.
- No backdrop blur, no glass, no gradient text, no blue-to-purple gradient. A soft radial glow on the voice orb is a state halo, not a decorative fill.
- `--motion-normal` is 200ms. Entrance uses `--ease-spring`. Exit uses `--ease-exit`. Pressed controls scale to `--tap-scale` (0.98), except the bottom navigation.
- Page changes on Android stay opacity-only. A translate or scale on the page root is painted squashed by the WebView.
- Respect `prefers-reduced-motion`. Every voice state stays distinguishable with animation off.

## Color

One chromatic accent: `--brand` (the warm metal). It marks the active tab, the focus ring, and the user side of a conversation. It is not a border around every control, a kicker, or a tinted icon tile.

Semantic colors are not extra accents:

- `--positive` for money in and completed gain
- `--negative` for money out, debt, and destructive confirmation
- `--warning` for battery optimization and a budget near its limit

Soft washes (`--positive-soft`, `--negative-soft`, `--brand-soft`) appear only inside a Badge or a Stat delta. Do not paint a whole card with them.

Voice states use `--state-listen`, `--state-speak`, `--state-think`, and `--state-error`. They alias brand, primary text, secondary text, and negative. Do not add a second accent for the orb.

Everything else is a neutral ramp:

| Token | Role |
| --- | --- |
| `--text-strong` | Rare emphasis inside a line |
| `--text-primary` | Titles and body |
| `--text-secondary` | Supporting lines |
| `--text-tertiary` | Labels, dates, hints |
| `--text-disabled` | Inactive only |
| `--on-brand` | Text on a brand fill. Aliases `--brand-ink` |

Surfaces, no pure white:

| Token | Role |
| --- | --- |
| `--surface-0` | Page |
| `--surface-1` | A card or a grouped region |
| `--surface-2` | A pressed or inset field, an assistant bubble |
| `--surface-3` | Card pressed, selected chip fill in dark themes |

Squint test: if a region collapses into one gold or pastel blob, the accent or a status color is being used as decoration. Remove it.

## Type

UI face: the system sans (`--font-sans`). Do not import a display serif to look expensive.

| Class | Token | Role |
| --- | --- | --- |
| `.t-display` | `--type-display` (3rem) | Hero figures |
| `.t-unit` | `--type-unit` (1.5rem) | Currency or unit beside a hero |
| `.t-headline` | `--type-headline` (1.75rem) | Screen title |
| `.t-title` | `--type-title` (1.25rem) | Card or sheet title |
| `.t-body` | `--type-body` (1rem) | Body, including Talk bubbles |
| `.t-label` | `--type-label` (0.875rem) | Supporting lines, buttons |
| `.t-caption` | `--type-caption` (0.8125rem) | Floor. Labels, captions |
| `.t-overline` | caption + wide tracking | A real group name, rare |
| `.t-num` | tabular numerals | Amounts |

Screen titles are sentence case, weight 500, tight tracking. Not condensed all-caps. Section labels are uppercase only when they name a real group.

Sizes are rem so WebView `textZoom` follows the Android font scale. Layouts grow with the text. Do not clip a label, a bubble, or a control at 130% or 200%. Words do not break mid-word. A region that would shrink to nothing keeps enough height to stay readable, and scrolls inside itself if it must.

`.text-xs` stays pinned to the 13px floor until a screen moves to `.t-caption`.

## Space

Spacing tokens: 4, 8, 12, 16, 24, 32, 48, 64 (`--space-1` through `--space-8`).

- Screen side padding is `--gutter` (16px, 24px from 600px).
- The important figure or the screen title sits at the top. Density increases downward.
- Do not stack the same section pattern. One figure, then a short line, then a list.
- Empty space is intentional. Do not fill it with a bordered card that repeats the title.

## Controls

- Primary: solid `--text-primary` fill, `--surface-0` label, `--radius-sm`, height at least `--touch`. No stroke.
- Secondary: text with an underline. Not an outlined rectangle.
- Destructive confirmation: solid `--negative`.
- Selected choice: the same solid fill, or a `--brand-soft` pill behind a nav icon. Not a gold outline on a card.
- Icons: lucide, 1.5px stroke, 20px inline and 24px in the nav, FAB, and app bar. Icon-only buttons have an `aria-label`. No tinted tile behind an icon.
- Do not use emoji as icons, “LIVE” badges, or status pills that repeat on every row.
- Sheets: drag handle, `--radius-lg` on the top corners, solid scrim, no blur. Always dismissable. Focus moves to the title and returns to the opener.

## Copy

One sentence under a title. Longer explanations go in an info tip. Name the thing and the next step.

Avoid, in all five locales (en, es, de, pt, ca-ES-valencia):

- “seamless”, “delve”, “leverage”, “robust”, “effortlessly”, and the same kind of filler
- three-adjective introductions
- “not just X, but Y”
- em dashes used to staple two clauses together
- Apostrophes in Android `strings.xml` are escaped (`\'`)

Voice names keep the vendor’s identity. Their blurbs stay to two beats.

## What already landed, and what v2 builds on

Pull request #132 (Talk bubbles, the landscape side rail, Activity, Sankey, and Plan) lands before the later visual phases. v2 builds on that work. It does not replace it.

- **Talk.** Keep the bubbles. A later phase restyles them with these tokens: `--surface-2` for the assistant bubble, an accent-tinted user bubble, the shared radius token, and `.t-body`. The voice orb and its states sit at the bottom, above the input, and replace the ring panel. The live caption is the streaming assistant bubble.
- **Shell.** The landscape rail is the Material navigation rail for the same five destinations as the portrait bar: Talk, Notes, Agenda, Finance, Settings. Same icons, same active indicator, same order.
- **Finance.** Keep that review’s Activity, Sankey, and Plan. The Overview “Money flow” preview card opens that Sankey full screen. Plan and Activity take the v2 card and list row. The hero net worth figure and its trend go on top of Overview.
- If that review clashes with a v2 token or the 48px target, keep its structure and fix the styling.

Those screens are not rebuilt in the phase that introduces this contract.

## Tokens

Themes set only:

`--surface-0`, `--surface-1`, `--surface-2`, `--surface-3`, `--text-primary`, `--text-tertiary`, `--text-mix`, `--brand`, `--brand-ink`, `--brand-deep`, `--on-brand`, `--positive`, `--negative`, `--warning`, `--line`, `--line-soft`, `--scheme`, `--elevation`.

Derived tokens (`--text-strong`, `--text-secondary`, `--text-disabled`, `--positive-soft`, `--negative-soft`, `--brand-soft`, `--state-listen`, `--state-speak`, `--state-think`, `--state-error`) and the legacy names (`--canvas`, `--panel`, `--ink`, `--gold`, `--sage`, `--rose`, `--shadow`, `--fin-*`) alias those. `--fin-*` are aliases of the semantic tokens, not a second palette. Do not put a raw hex in a component. Charts included: they read the aliases.

## Screen checklist

Before calling a screen done:

1. No full outline around a card, chip, or secondary button.
2. No corner brackets, glass, gradient fills, or emoji.
3. One accent on the screen, used once or twice, plus positive and negative for money.
4. No nested cards.
5. Title is sentence case. At most one sentence under it.
6. Text is at least 13px and still readable at 130% and 200% font scale, with nothing clipped.
7. Targets are at least 48px, and their hit areas do not overlap.
8. The first screenful has a clear top (title or figure) and a quieter bottom.
9. Motion uses the tokens. Reduced motion still reads.
10. Strings exist in en, es, de, pt, and ca-ES-valencia.
