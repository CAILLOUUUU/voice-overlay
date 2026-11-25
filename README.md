# VOICE OVERLAY - CAILLOU FEAT CLAUDE

🇬🇧 **English version: [README.en.md](README.en.md)**

Overlay OBS qui affiche les personnes présentes dans **ton** salon vocal Discord, et qui te suit tout seul quand tu changes de salon. Aucun salon à choisir à la main, rien à installer sur le serveur.

![Page de réglages avec aperçu en direct](docs/settings.png)

- Montre qui parle, qui est mute ou sourd, avec les décorations d'avatar Discord
- Suit le vocal où tu te trouves, et n'affiche rien quand tu n'es dans aucun
- Renommer, réordonner ou masquer des personnes à la volée, sans tout perdre si Discord plante
- Page de réglages en direct : disposition, couleurs, 12 polices fournies (ou importe la tienne), CSS personnalisé
- Rôles League of Legends (Top, Jungle, Mid, ADC, Support) détectés depuis le client League quand le pseudo ressemble, ou choisis à la main, avec tri automatique
- Inhouses [DPM](https://inhouse.dpm.lol) : avec l'extension Chrome fournie (dossier `extension`), rôle et équipe de chaque joueur, et lien Discord ↔ League appris tout seul quand les pastilles passent au vert
- Plusieurs profils d'overlay utilisables en même temps (par exemple vertical en jeu, horizontal ailleurs), chacun avec son lien OBS
- Exporte ton style dans un fichier pour le partager
- En français et en anglais

> **À savoir :** l'appli dépend d'une fonction de Discord (RPC local) encore classée en bêta privée. Si Discord la modifie ou la ferme, l'overlay cessera de fonctionner. Chaque utilisateur doit aussi créer sa propre « application » Discord gratuite (l'appli te guide, environ 2 minutes).

## Installation (Windows)

1. Télécharge `Voice-Overlay-<version>.exe` (par exemple `Voice-Overlay-1.2.0.exe`) depuis la page Releases et mets-le où tu veux (Téléchargements, Bureau…). Il ne crée aucun fichier à côté de lui.
2. Ouvre Discord **desktop** (pas la version navigateur), puis lance le `.exe`.
   - Windows peut afficher « Windows a protégé votre ordinateur » : clique sur **Informations complémentaires**, puis **Exécuter quand même**. C'est normal pour une appli non signée.
3. Un assistant s'ouvre dans ton navigateur et te guide en 4 étapes pour relier l'overlay à Discord.
4. À la fin, Discord ouvre une fenêtre : clique sur **Autoriser**.

L'appli tourne ensuite en arrière-plan. Son icône est à côté de l'horloge : clic gauche pour les réglages, clic droit pour quitter.

## Dans OBS

Ajoute une **Source → Navigateur** avec le lien du profil, affiché dans les réglages (par exemple `http://localhost:47474/?profile=principal`). Largeur 400, hauteur 600 pour commencer. Le fond est transparent.

Chaque profil a son propre lien : crée-en un par scène et ajoute chaque lien dans la scène qui va avec. Ils fonctionnent tous en même temps.

Pour des ajustements rapides, des options dans l'URL passent au-dessus des réglages du profil :
`?title` `?noTitle` `?hideSelf` `?speakingOnly` `?noName` `?size=48` `?align=right` `?layout=row`

## Vie privée

- Tout tourne sur ton PC. Tes réglages et tes codes Discord sont enregistrés dans `%APPDATA%\VoiceOverlayCaillou` (bouton « Ouvrir » dans la partie Application des réglages). Ne partage jamais `config.json` ni `token.json` et ne les montre pas en stream.
- Les seules connexions extérieures vont vers Discord (connexion, avatars) et vers GitHub, pour vérifier s'il existe une version plus récente.

## En cas de souci

Le fichier `voice-overlay.log`, dans le dossier des données (`%APPDATA%\VoiceOverlayCaillou`), dit ce qui s'est passé.

Pour tout effacer et repartir de zéro : quitte l'appli et supprime ce dossier.

## Développement

Rien à installer pour le lancer, il faut seulement Node.js 20 ou plus :

```
node server.js --open     # mode normal (Discord desktop ouvert)
node server.js --demo     # faux membres, sans Discord
```

| Fichier | Rôle |
|---|---|
| `server.js` | Connexion à Discord (RPC local), serveur local, réglages |
| `overlay.html` | La page affichée dans OBS |
| `settings.html` | La page de réglages et l'assistant |
| `webfonts/` | Polices fournies (licence OFL) |
| `build/` | Fabrication du `.exe` (`cd build && npm install && node build.js`) |

## Crédits

Fait par Caillou, construit avec Claude. Publié sous [licence MIT](LICENSE). Embarque Node.js et des polices libres : voir [LICENCES-TIERS.txt](LICENCES-TIERS.txt).
