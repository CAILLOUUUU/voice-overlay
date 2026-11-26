# Design

The settings app (`settings.html`) is a sibling of [dpm.lol](https://dpm.lol) and [inhouse.dpm.lol](https://inhouse.dpm.lol): same palette, font and card language, so League inhouse players feel at home. The overlay shown in OBS is styled by the user and is not part of this system.

## Palette

| Token | Value | Use |
|---|---|---|
| `--bg` | `#131619` | Page ground (faint periwinkle glow at the top) |
| `--bar` | `#0d0f10` | Top bar |
| `--panel` / `--panel2` | `#1b1e25` / `#232731` | Cards / secondary buttons, active chips |
| `--field` | `#171a20` | Inputs |
| `--line` / `--line2` | white 8% / 14% | Hairlines / control borders |
| `--text` / `--dim` / `--faint` | `#f3f4f8` / `#9b9ea6` / `#686b72` | Text levels |
| `--accent` / `--accent2` | `#a6b0f2` / `#7989ec` | Selection, links, values, default buttons (dark text) |
| `--gold` | `#ffd47d` | The one primary action per view (dark text); renamed people and manual roles |
| `--ok` / `--warn` / `--bad` | `#4ac97e` / `#ffb547` / `#f7665e` | Talking, waiting, muted/errors/destructive |

## Type

Plus Jakarta Sans (variable, self-hosted in `webfonts/plus-jakarta-sans-var.woff2`, OFL) for everything. 800 for headings and tab labels, 700 for names and buttons, 500 body. Tabular figures throughout. Monospace only for links and CSS.

## Components

- **Cards**: 12px radius, hairline border, 3px periwinkle gradient bar along the bottom edge (DPM's signature). Collapsible cards use `<details>` with a chevron.
- **Buttons**: periwinkle (default), gold `.primary` (one per view), `.ghost` (panel2 + hairline), `.danger` turns red on hover and while confirming (two-click confirm for irreversible actions).
- **Tabs**: text tabs with a periwinkle underline; count badge is a small periwinkle tag, green while someone talks.
- **People leaderboard** (signature): rows like a DPM ranking. Order number (becomes a drag grip on hover), avatar with green ring when talking, bold editable name (gold when renamed), a meta line with pills (Talking, Muted/Deafened) and the Riot ID in periwinkle, role select (gold border when set by hand), eye toggle for hiding (struck name + crossed eye, readable without colour).
- **Preview**: the real overlay page at the real OBS source size (400 × 600, dashed outline, scaled to fit) on a choice of backgrounds. Clicking a person in it jumps to their row.

## Motion

150–250 ms colour/border transitions; a one-off highlight flash when jumping to a person. No decorative motion.
