# build-tokens

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`assets/app.css` defines every design token of HANDOFF section 4 once on `:root`, with dark, print, forced-colors and reduced-motion blocks. A Rust test keeps the values equal to HANDOFF.md and checks every token pair for contrast: 4.5:1 for text and 3:1 for UI. A lint refuses a raw number in the properties that take tokens only. Later cards use the tokens and add none: a value no token covers is a gap, which the UI lane closes in HANDOFF.

## Why

- Tokens on `:root`, the dark, print and forced-colors blocks, system fonts only and the contrast test are decided in [AD-15](../decisions/0001-foundation.md#ad-15-ui-technology-and-design-system).
- Design: the [MAP.md](../design/MAP.md#design-tokens) section "Design tokens". [HANDOFF.md §4 Design tokens](../design/HANDOFF.md#4-design-tokens) owns every value, the dark-theme rule and the forced-colours rule; [Using the tokens](../design/HANDOFF.md#using-the-tokens) says which properties take only tokens; [§12 Implementation notes](../design/HANDOFF.md#12-implementation-notes) (Tokens, Testing, the brand layer). Token groups: colour, type, space, shape and line, size, motion, layering and elevation (all of them), with the Print, Forced colours and Reduced motion tables.

## Scope

- Files: `crates/fieldbook-render/assets/app.css`, `crates/fieldbook-render/tests/tokens.rs`.
- The token-use lint lives in `tokens.rs`, so it runs in the same test binary as the contrast test.
- Docs to update: none. The token table stays in HANDOFF.md; this card does not copy it into another document.

## Out of scope

- Component rules and layout (`build-frame` and later). This card writes only the token blocks and the `body` defaults that use them.
- The theme toggle and its stored choice (`build-frame`).
- The brand layer. None ships unless the owner supplies one.
- The print layout and footer (`day-one-binder-and-registers-xlsx`). The print block here sets token values only (`--color-bg` and the two shadows).

## Preconditions

- Cards merged: `build-v0`.
- This card touches only `app.css` and its test, so it may run beside `build-site-lints`.

## Steps

1. Replace the placeholder in `app.css` with the token blocks, in this order:
   - `:root`: every token of every HANDOFF §4 table (Colour, Type, Space, Shape and line, Size, Motion, Layering, Elevation) with its light or single value.
   - `@media screen and (prefers-color-scheme: dark)` around `:root:not([data-theme="light"])`, then `@media screen` around `:root[data-theme="dark"]`: the dark value of every token that has one (the Colour and Elevation tables), and `color-scheme: dark`. Dark applies on screen only, so print starts from the light values.
   - `@media print`: only `--color-bg` as `#ffffff`, `--shadow-raise` as `none` and `--shadow-pop` as `none` (the Print table).
   - `@media (forced-colors: active)`: the Forced colours table, on the selectors `:root, :root:not([data-theme="light"]), :root[data-theme="dark"]`, so it wins over both dark blocks. Chips and banners keep their icon and text.
   - `@media (prefers-reduced-motion: reduce)`: `--dur-fast`, `--dur-med` and `--dur-pulse` as `0ms` (the Reduced motion table). `--dur-flash` is not changed.
   - `@media (max-width: 767px) { :root { --drawer-w: 100vw; } }`: the phone override, retaining the base `--drawer-w: 440px`. No other custom property is allowed in this block.
2. Add the `body` defaults: `--color-fg` on `--color-bg`, in `--font-ui` at `--text-md` with `--leading-ui`. Nothing else. `--text-read` with `--leading-read` is for reader prose, which the page cards set.
3. Write `tests/tokens.rs` with `mod tokens`. Read `app.css` with `include_str!` and parse the blocks with a small hand-written reader of `--name: value;` lines.
   - `values_match_handoff`: read every table of HANDOFF §4 whose header row starts `| Token |` with `include_str!`, and require the same names and values in the matching block of `app.css`. The second column says which block: Light and Dark (`:root` and the dark blocks), Value (`:root`), Print, Forced colours or Reduced motion. Skip a brand-layer table if one appears. The Breakpoints table, whose header starts `| Query |`, is not a token table. If the MAP.md Design tokens section still holds a token table, it must agree too. A difference names the token.
   - `each_token_defined_once_per_block`; `every_var_is_defined` (every `var(--name)` in `app.css` names a token defined on `:root`); `dark_blocks_are_equal`; `no_custom_property_outside_the_blocks`; `no_colour_literal_outside_the_blocks`.
   - `text_pairs_reach_4_5`: every text token (`fg`, `muted`, `accent`, and the four status colours) on `bg`, `surface` and `surface-2`; each status colour on its `-soft` fill; `accent-fg` on `accent` and `accent-hover`; `fg` on `mark`; in light and dark. A failure prints the pair, the theme and the ratio.
   - `token_use_lint`: in every rule of `app.css` outside the token blocks, the properties that take tokens only (font family, size, weight and line height, letter spacing, padding, margin, gap, border and outline widths, outline offset, radius, box shadow, z-index, and transition and animation durations) hold no raw number. The literals `0`, `auto`, `inherit`, `none`, `normal`, a percentage and the `50%` of a circle are allowed; `calc()` of tokens is allowed. Width, height, sizes, grid tracks, positions, transforms, SVG attributes and the query widths of Breakpoints stay literal. A failure names the rule and the property.
   - `ui_pairs_reach_3`: `focus`, `chart-1`, `chart-neutral` and `muted` (the input outline) on `bg`, `surface` and `surface-2`, in light and dark.
   - `no_web_font`: no `@font-face` and no `url(`.

4. In `tokens::phone_drawer_override_matches`, require the phone block at exactly max-width 767px with only --drawer-w: 100vw and the base 440px value. Feed the block checker fixtures with a missing override, wrong breakpoint, wrong value and unrelated custom property; each must fail. The general component custom-property prohibition remains.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render tokens::
just site
cmp crates/fieldbook-render/assets/app.css out/edition/assets/app.css
! grep -n -E '@font-face|url\(' crates/fieldbook-render/assets/app.css
grep -c 'forced-colors: active' crates/fieldbook-render/assets/app.css
grep -c 'prefers-reduced-motion: reduce' crates/fieldbook-render/assets/app.css
just check
```

Expected: the tests pass; `just site` exits 0; `cmp` prints nothing; the guarded `grep` exits 0; the `forced-colors` and `prefers-reduced-motion` counts are 1; `just check` exits 0.

## Negative control

Set the light `--color-muted` to `#9aa3b0`. `tokens::text_pairs_reach_4_5` must fail and name `muted` on `surface` with its ratio, and `tokens::values_match_handoff` must fail and name `--color-muted`. Restore the file. Then add `--color-extra: #000000;` inside a component rule: `tokens::no_custom_property_outside_the_blocks` must fail. Remove it. Then add `padding: 7px;` to a component rule: `tokens::token_use_lint` must fail and name the rule. Remove it. Then delete `--dur-pulse` from the reduced-motion block: `tokens::values_match_handoff` must fail and name it. Restore the file.

Remove or alter the phone override, or add an unrelated custom property to it: `tokens::phone_drawer_override_matches` must fail. Restore each.

## Rollback

Revert the merge commit. `app.css` returns to its placeholder. Revert every later `build-*` card first, since they use the tokens.

## BLOCKED

- A HANDOFF §4 pair fails its contrast rule: name the pair, the theme and the ratio. Do not change a value; the UI lane owns them.
- A HANDOFF §4 row gives no single value that CSS can hold, such as a token range with fewer values than names, or a shadow given only in words: name the row and stop. The lead asks the UI lane; do not choose a value.
