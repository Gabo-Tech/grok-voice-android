# Grok Voice design language

Read this before changing any screen, sheet, widget, or theme. It is the contract for agents. Themes change tokens. They do not invent new colors, radii, or shadows in a component.

The app is a phone instrument: Talk, Notes, Calendar, Time, Finance, Settings. It should feel like a quiet luxury object. The references are the My Porsche app and Chanel’s digital surfaces, not a SaaS dashboard and not a magazine template.

## Luxury reference

**Porsche (My Porsche).** Near-black or warm-gray fields, never pure black. Type does the work: a tightly tracked sans for titles, small labels only where a group needs a name. Content sits on the canvas. Groups separate by a shift in tone or by a hairline, not by a stroked box. Icons are thin. One accent exists and is used as a precise mark (a selected tab, a focus line), not as a frame around every control. Primary actions are solid fills. Secondary actions are text.

**Chanel.** Editorial restraint. Generous empty space. The name and the section title are the loudest things on the screen. Labels, when they appear, are small, widely tracked, and rare. Links and secondary actions are underlined type, not pills. Radius is sharp or barely there. Nothing is decorated with corner ornaments, gradients, or glass.

What we take from both: typography-led hierarchy, tonal surfaces, hairline dividers, one metal accent, solid or text controls, and empty space as a material.

## Floors that do not move

- No pure black (`#000`) and no pure white (`#FFF`) in surfaces or text. Near-black and near-white only.
- Minimum text size is 13px. That includes navigation labels, captions, and calendar counts.
- Touch targets are at least 44px. A thin icon still sits in a 44px hit area.
- Text on the canvas meets WCAG AAA where the existing palette tests require it (7:1 for primary text, 4.5:1 for muted text). Borders that carry meaning meet 3:1. Do not communicate state by color alone.
- Themes are Graphite, Ink, Stone, Midnight, Carbon, Ivory, Sand, Forest, Plum, System, and Custom. One `data-appearance` attribute on `<html>` swaps the theme. Custom paints the same semantic tokens from script.
- Dark themes are the default posture (Graphite). Light themes are Stone, Ivory, and Sand.

## Override of the old chrome rules

Older builds drew a 1px outline and an 8px radius on every card, chip, and button, plus gold corner brackets. That reads as cheap. It is retired.

- **Radius.** `--radius` is 2px. `--radius-sharp` is 0. `--radius-soft` is 4px, and only for the top corners of a sheet. Do not use 8px, 12px, or pill shapes on cards, chips, or buttons. Circles are allowed for the talk ring and the switch knob, because those controls are round, not rounded rectangles.
- **Borders.** A full outline around a region is not the default. Separate content with space, a tonal step (`--surface-0` to `--surface-1`), or a hairline (`--line-soft`) on one edge. A real border (`--line`, 1px) is for a text field’s baseline, a destructive confirmation, or keyboard focus.
- **Corner brackets.** Do not draw decorative corner marks on panels, charts, or cards.

## Color

One chromatic accent: `--brand` (the existing warm metal). It marks the active tab and the focus ring. It is not a border, a kicker, an icon tile, or a background wash.

Semantic colors are not extra accents:

- `--positive` for money in and completed gain
- `--negative` for money out, debt, and destructive confirmation

Everything else is a five-step neutral:

| Token | Role |
| --- | --- |
| `--text-strong` | Rare emphasis inside a line |
| `--text-primary` | Titles and body |
| `--text-secondary` | Supporting lines |
| `--text-tertiary` | Labels, dates, hints |
| `--text-disabled` | Inactive only |

Surfaces, lightest separation, no pure white:

| Token | Role |
| --- | --- |
| `--surface-0` | Page |
| `--surface-1` | A grouped region that needs a tonal step |
| `--surface-2` | A pressed or inset field |

A card earns `--surface-1` only when the group is a different object from the page (a sheet, a custom-theme preview). A list of settings does not each get a card. Nested cards are a defect. If a surface sits on a surface, remove the inner one.

Squint test: if a region collapses into one gold or pastel blob, the accent or a status color is being used as decoration. Remove it.

## Type

- UI face: the system sans. Do not import a display serif to look expensive.
- Screen titles (`.masthead`): sentence case, weight 500, tracking about `-0.035em`. Not condensed all-caps.
- Section labels (`.chapter`): 13px, weight 500, wide tracking, uppercase. Use them for a real group name, not for every button.
- Body: 16px. Secondary copy: 14px. Nothing under 13px.
- Hero figures use a 2:1 number-to-unit ratio. A 48px amount takes a 24px unit (`--text-display` / `--text-unit`), baseline-aligned, with the number first. Tabular numerals. Do not set the currency and the amount in one identical size.

## Space and rhythm

Spacing tokens: 4, 8, 12, 16, 24, 32, 48, 64 (`--space-1` through `--space-8`).

- Do not stack the same section pattern. A tall quiet header is followed by a denser list, not by another framed card of the same weight.
- The important figure or the screen title sits at the top. Density increases downward. The bottom of a screen is for lists, not for a second hero.
- Identical KPI tiles in a row are a defect. Vary the content: one figure, then a short line, then a list.
- Empty space is intentional. Do not fill it with a bordered card that repeats the title.

## Elevation and motion

- Dark themes: `--elevation: none`. Depth is a lighter surface and a hairline.
- Light themes: a shadow no heavier than 4% black, `0 1px 2px`. No hard offset shadows, no glow.
- No backdrop blur, no glass, no gradient text, no blue-to-purple gradient.
- `--motion-normal` is 200ms. Entrance uses `--ease-spring`. Exit uses `--ease-exit` (ease-out). Pressed controls scale to `--tap-scale` (0.98), except the bottom navigation.
- Page changes on Android stay opacity-only. A translate or scale on the page root is painted squashed by the WebView. Do not “fix” that with a slide.
- Respect `prefers-reduced-motion`.

## Controls

- Primary: solid `--text-primary` fill, `--surface-0` label, 2px radius, no stroke.
- Secondary: text with an underline. Not an outlined rectangle.
- Selected choice: the same solid fill, or a 1px `--brand` mark on a tab. Not a gold outline.
- Icons: 1.5px stroke, bare. No tinted tile behind an icon. If the word is clear, omit the icon.
- Do not use emoji as icons, “LIVE” badges, or status pills that repeat on every row.
- Sheets: a hairline at the top, 4px top radius, solid scrim, no blur. Always dismissable.

## Copy

Short, specific, spoken English. Name the thing and the next step.

Avoid, in all five locales (en, es, de, pt, ca-ES-valencia):

- “seamless”, “delve”, “leverage”, “robust”, “effortlessly”, and the same kind of filler
- three-adjective introductions (“elegant, refined, and premium”)
- “not just X, but Y”
- em dashes used to staple two clauses together. Use a period or a comma.
- Apostrophes in Android `strings.xml` are escaped (`\'`).

Voice names keep the vendor’s identity. Their blurbs stay to two beats, not a stack of adjectives.

## Tokens

Themes set only:

`--surface-0`, `--surface-1`, `--surface-2`, `--text-primary`, `--text-tertiary`, `--text-mix`, `--brand`, `--brand-ink`, `--brand-deep`, `--positive`, `--negative`, `--line`, `--line-soft`, `--scheme`, `--elevation`.

Derived tokens (`--text-strong`, `--text-secondary`, `--text-disabled`) and the legacy names (`--canvas`, `--panel`, `--ink`, `--gold`, `--sage`, `--rose`, `--shadow`) alias those. Do not put a raw hex in a component. Charts included: they read the aliases.

## Screen checklist

Before calling a screen done:

1. No full outline around a card, chip, or secondary button.
2. No corner brackets.
3. One accent on the screen, used once or twice.
4. No nested cards.
5. Title is sentence case. Labels in wide caps are rare.
6. Text is at least 13px. Targets are at least 44px.
7. The first screenful has a clear top (title or figure) and a quieter bottom.
8. Motion uses the tokens. Reduced motion still reads.
9. Strings exist in en, es, de, pt, and ca-ES-valencia.

## Where this disagrees with the source notes

StyleSeed’s sample icon badge (a brand-tinted tile at 10% opacity) and its 8–16px radii are not used here. Those are the tells the de-slop notes call out, and they fight the Porsche/Chanel reference. StyleSeed’s useful parts stand: semantic tokens, one accent, five neutrals, 2:1 figures, rhythm that does not repeat a section, elevation instead of heavy shadow, and motion with a spring in and an ease-out. De-slop’s bans stand: tinted icon boxes, color goo, glass, nested cards, heavy shadows, emoji, live-badges, gradient fills, and generic copy.
