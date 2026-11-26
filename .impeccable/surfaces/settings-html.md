---
version: 1
slug: "settings-html"
primary_target: "settings.html"
related_targets: []
---

# Settings app (settings.html)

Mode: Operate. Audience: League streamers and DPM inhouse organizers, mostly using it live mid-stream (see PRODUCT.md).
Direction: user-pinned "make it like dpm.lol" (overrides the roll). Unresolved: none.

## Direction contract

THESIS: The settings app is a sibling of dpm.lol and inhouse.dpm.lol, not a dev tool: the people in voice read like a DPM leaderboard you can edit live. It refuses the grey-panel-with-purple-accent settings page and any neon gamer glow.

OWN-WORLD: DPM's palette: blue-black ground (#131619), darker top bar (#0d0f10), raised cards (#1d2028) with 10% white hairlines, near-white text, grey secondary. Periwinkle (#a6b0f2 / #7989ec) for selection, links and values; DPM gold (#ffd47d, dark text) only for the one primary action per view; DPM red/green for states. Plus Jakarta Sans everywhere, bold for names and numbers, tabular figures. 4-6px control radii, 12px cards with a coloured bottom bar.

STORY: The streamer glances at who is in voice, fixes a name, role or visibility in one click, and leaves. Look and OBS setup live one tab away; rare settings fold away.

FIRST VIEWPORT: Top bar: name, live status pill, language. Left: tabs (People, Style, Profile & OBS, App) over a leaderboard table of people (order number, avatar, name with Riot ID subline, role badge, talking pill, hide eye). Right: profile chips over the preview, framed at the real OBS source size; clicking a person in the preview jumps to their row.

FORM: user-pinned dpm.lol world (not on the grounded list; roll assigned candidate 3, "On Air", overridden by the user). seed:3587c5eed4f9454a

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
