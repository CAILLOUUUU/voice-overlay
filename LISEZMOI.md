# VOICE OVERLAY - CAILLOU FEAT CLAUDE

🇬🇧 **English version: [README.md](README.md)**

Overlay OBS qui affiche les personnes présentes dans **ton** salon vocal Discord, et qui te suit tout seul quand tu changes de salon. Aucun salon à choisir à la main, rien à installer sur le serveur.

- Montre qui parle, qui est mute ou sourd, avec les décorations d'avatar Discord
- Suit le vocal où tu te trouves, et n'affiche rien quand tu n'es dans aucun
- Renommer, réordonner ou masquer des personnes à la volée (oublié quand elles partent)
- Page de réglages en direct : disposition, couleurs, 12 polices fournies (ou importe la tienne), CSS personnalisé
- Exporte ton style dans un fichier pour le partager
- En français et en anglais

> **À savoir :** l'appli dépend d'une fonction de Discord (RPC local) encore classée en bêta privée. Si Discord la modifie ou la ferme, l'overlay cessera de fonctionner. Chaque utilisateur doit aussi créer sa propre « application » Discord gratuite (l'appli te guide, environ 2 minutes).

## Installation (Windows)

1. Mets `Voice Overlay - CAILLOU FEAT CLAUDE.exe` dans un dossier à toi (il enregistre ses réglages à côté de lui).
2. Ouvre Discord **desktop** (pas la version navigateur), puis lance le `.exe`.
   - Windows peut afficher « Windows a protégé votre ordinateur » : clique sur **Informations complémentaires**, puis **Exécuter quand même**. C'est normal pour une appli non signée.
3. Un assistant s'ouvre dans ton navigateur et te guide en 4 étapes pour relier l'overlay à Discord.
4. À la fin, Discord ouvre une fenêtre : clique sur **Autoriser**.

L'appli tourne ensuite en arrière-plan. Son icône est à côté de l'horloge : clic gauche pour les réglages, clic droit pour quitter.

## Dans OBS

Ajoute une **Source → Navigateur** avec l'URL `http://localhost:47474/` (largeur 400, hauteur 600 pour commencer). Le fond est transparent.

Pour avoir deux sources avec des styles différents, des options dans l'URL passent au-dessus des réglages :
`?title` `?noTitle` `?hideSelf` `?speakingOnly` `?noName` `?size=48` `?align=right` `?layout=row`

## Vie privée

- Tout tourne sur ton PC. Tes codes Discord sont enregistrés à côté du `.exe` dans `config.json` et `token.json` : ne partage jamais ces fichiers et ne les montre pas en stream.
- Les seules connexions extérieures vont vers Discord (connexion, avatars) et vers GitHub, pour vérifier s'il existe une version plus récente.

## En cas de souci

Le fichier `voice-overlay.log`, à côté du `.exe`, dit ce qui s'est passé.

## Crédits

Fait par Caillou, construit avec Claude. Embarque Node.js et des polices libres : voir [LICENCES-TIERS.txt](LICENCES-TIERS.txt).
