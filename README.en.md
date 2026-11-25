# VOICE OVERLAY - CAILLOU FEAT CLAUDE

🇫🇷 **Version française : [README.md](README.md)**

An OBS overlay that shows who is in **your** Discord voice channel, and follows you automatically when you switch channels. No channel to pick by hand, nothing to install on the server.

![Settings page with live preview](docs/settings.png)

- Shows who is speaking, muted or deafened, with Discord avatar decorations
- Follows whatever voice channel you are in, and shows nothing when you are in none
- Rename, reorder or hide people on the fly, without losing it all if Discord crashes
- Live settings page: layout, colors, 12 included fonts (or import your own), custom CSS
- League of Legends roles (Top, Jungle, Mid, ADC, Support) detected from the League client when names look alike, or picked by hand, with automatic sorting
- [DPM](https://inhouse.dpm.lol) inhouses: with the included Chrome extension (`extension` folder), each player's role and team, and the Discord ↔ League link learned automatically as players join the voice channel
- Several overlay profiles usable at the same time (for example vertical in game, horizontal elsewhere), each with its own OBS link
- Export your style to a file and share it
- English and French

> **Good to know:** this app relies on a Discord feature (local RPC) that Discord still labels as a private beta. If Discord changes or closes it, the overlay will stop working. Each user also needs to create their own free Discord "application" (the app walks you through it, about 2 minutes).

## Install (Windows)

1. Download `Voice-Overlay-<version>.exe` (for example `Voice-Overlay-1.2.0.exe`) from the Releases page and put it wherever you like (Downloads, Desktop…). It creates no files next to itself.
2. Open the Discord **desktop** app, then run the `.exe`.
   - Windows may show "Windows protected your PC": click **More info**, then **Run anyway**. This is expected for an unsigned app.
3. A setup assistant opens in your browser and guides you through 4 steps to link the overlay to Discord.
4. At the end, Discord opens a window: click **Authorize**.

The app then runs in the background with an icon next to the clock: left-click for settings, right-click to quit.

## In OBS

Add a **Browser** source with the profile's link, shown in the settings (for example `http://localhost:47474/?profile=principal`). Width 400, height 600 to start. The background is transparent.

Each profile has its own link: create one per scene and add each link to its scene. They all work at the same time.

For quick tweaks, URL options override the profile's settings:
`?title` `?noTitle` `?hideSelf` `?speakingOnly` `?noName` `?size=48` `?align=right` `?layout=row`

## Privacy

- Everything runs on your PC. Your settings and Discord codes are stored in `%APPDATA%\VoiceOverlayCaillou` ("Open" button in the Application section of the settings). Never share `config.json` or `token.json` or show them on stream.

If something goes wrong, `voice-overlay.log` in that folder explains what happened. To reset everything, quit the app and delete that folder.
- The only outside requests are to Discord (login, avatars) and to GitHub, to check whether a newer version exists.

## Development

No dependencies to run, only Node.js 20 or later:

```
node server.js --open     # normal mode (Discord desktop open)
node server.js --demo     # fake members, no Discord needed
```

| File | Role |
|---|---|
| `server.js` | Discord connection (local RPC), local server, settings |
| `overlay.html` | The page shown in OBS |
| `settings.html` | The settings page and setup assistant |
| `webfonts/` | Included fonts (OFL license) |
| `build/` | Builds the `.exe` (`cd build && npm install && node build.js`) |

## Credits

Made by Caillou, built with Claude. Released under the [MIT License](LICENSE). Bundles Node.js and open fonts: see [LICENCES-TIERS.txt](LICENCES-TIERS.txt).
