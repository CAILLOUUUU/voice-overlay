# VOICE OVERLAY - CAILLOU FEAT CLAUDE

Overlay OBS qui affiche qui est dans ton salon vocal Discord, et qui suit tout seul le vocal où tu te trouves.

- Guide d'utilisation : [LISEZMOI.md](LISEZMOI.md)
- Téléchargement : `Voice Overlay - CAILLOU X CLAUDE.exe` dans les Releases

## Développement

Aucune dépendance pour lancer le programme, il faut seulement Node.js 20 ou plus :

```
node server.js --open     # mode normal (Discord desktop ouvert)
node server.js --demo     # faux membres, sans Discord
```

| Fichier | Rôle |
|---|---|
| `server.js` | Connexion à Discord (RPC local), serveur local, réglages |
| `overlay.html` | La page affichée dans OBS |
| `settings.html` | La page de réglages |
| `webfonts/` | Polices fournies (licence OFL) |
| `build/` | Fabrication de `Voice Overlay - CAILLOU X CLAUDE.exe` |

## Fabriquer le .exe

```
cd build
npm install
node build.js
```

Le résultat arrive dans `dist/Voice Overlay - CAILLOU X CLAUDE.exe`.

## Ne jamais publier

`config.json` et `token.json` contiennent des clés privées. Ils sont exclus par `.gitignore`.
