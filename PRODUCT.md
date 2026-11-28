# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

League of Legends streamers, and organizers or streamers of League inhouse games run on inhouse.dpm.lol. They stream with OBS while sitting in a Discord voice channel with their team. They open the settings app once to set up the look, then mostly come back to it **live, mid-stream**, to rename, hide, reorder people or fix a role, usually with the game running and little attention to spare.

## Product Purpose

Voice Overlay shows who is in the streamer's own Discord voice channel, who is speaking, muted or deafened, inside an OBS browser source, and follows the streamer automatically when they change channels. Success: the overlay just works once set up, and any live correction takes a few seconds without leaving the game for long.

## Positioning

It follows *your* voice channel automatically (no channel to pick), and it knows League: roles detected from the League client or from a DPM inhouse lobby, team sides, and the Discord ↔ League account links learned as players join voice. Nothing else on the market does this for inhouses.

## Operating Context

- Runs locally on Windows (Rust `.exe`, tray icon). The settings open in their own app window (WebView2), also reachable at `http://localhost:47474/settings`.
- The overlay is a separate page shown by OBS as a transparent browser source; the settings preview embeds that same page, so the preview is exactly what OBS shows.
- Used next to Discord, OBS, the League client and sometimes the DPM site in Chrome (with the bridge extension).

## Capabilities and Constraints

- Settings capabilities to keep: Discord setup assistant (4 steps), several overlay profiles each with its own OBS link, live preview with background choices, ready-made styles, every style option (title, layout, size, gap, shape, names, icons, decorations, roles, hide self, speaking only, dim silent, effect, colors, opacity, font incl. imported fonts, bold, animation, custom CSS), style export/import, people list (rename, reorder by drag, hide, manual role, League/DPM links), forget delay and resets, in-game overlay (own style, League games only, click-through, excluded from capture, hotkey to move), autostart, data folder, diagnostics, quit, one-click update, French and English.
- `settings.html` is one self-contained file (no build step, no framework), served unchanged by both the Node version and the Rust app, and embedded in the `.exe`. Fonts are self-hosted under `webfonts/` (OFL).
- The overlay page's look is user-configured and separate from the settings app's look.

## Brand Commitments

- Name stays: "Voice Overlay · Caillou feat Claude". The icon may change with the redesign.
- Must not look like a generic "gamer RGB" app (neon glow, aggressive angles, fake esports flash), and must not imitate the League/Riot client look.

## Evidence on Hand

Real UI copy in French and English inside `settings.html`; app icon `icon.png` / `icon.ico`; 12 bundled fonts in `webfonts/`. No testimonials, user counts or press exist; do not invent any.

## Product Principles

1. Live corrections come first: the people list must be the fastest thing to reach and use, never slower than today.
2. The preview is the truth: always the real overlay, never a re-drawing.
3. Nothing that works today disappears in a redesign.
4. Set-and-forget settings stay out of the way until needed.
5. Plain language, bilingual, no jargon.
