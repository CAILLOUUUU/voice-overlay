# VOICE OVERLAY - CAILLOU FEAT CLAUDE

Overlay OBS qui affiche les personnes présentes dans **ton** salon vocal Discord, quel qu'il soit.
Il suit automatiquement le vocal où tu te trouves : quand tu changes de salon, l'overlay change tout seul.
Quand tu n'es dans aucun vocal, il n'affiche rien.

## Installation (une seule fois)

1. Mets **Voice Overlay.exe** dans un dossier à toi (il y enregistre ses réglages).
2. Ouvre Discord **desktop** (pas la version navigateur), puis lance **Voice Overlay.exe**.
   - Windows peut afficher « Windows a protégé votre ordinateur » : clique sur **Informations complémentaires**, puis **Exécuter quand même**. C'est normal pour une appli non signée.
3. La page de réglages s'ouvre. Suis les 4 étapes pour relier l'overlay à Discord (création d'une appli Discord gratuite, environ 2 minutes).
4. Une fenêtre s'ouvre dans Discord : clique sur **Autoriser**.

L'appli tourne ensuite en arrière-plan, sans fenêtre. Son icône est à côté de l'horloge :
clic gauche pour ouvrir les réglages, clic droit pour quitter.

## Dans OBS

Ajoute une **Source → Navigateur** :
- URL : `http://localhost:3000/`
- Largeur 400, hauteur 600 pour commencer

## Réglages (`http://localhost:3000/settings`)

Tout s'applique en direct dans OBS :
- **Personnes dans le vocal** : renommer quelqu'un, changer l'ordre, masquer quelqu'un. Ces changements sont oubliés quand la personne quitte ton vocal.
- **Style** : titre (`{salon}` = nom du vocal, `{nb}` = nombre de personnes), disposition, couleurs, police (12 polices fournies, ou importe la tienne), effet quand quelqu'un parle, CSS personnalisé.
- **Application** : lancer avec Windows, quitter.

### Deux sources OBS avec des styles différents
Des options dans l'URL passent au-dessus des réglages :
`?title` `?noTitle` `?hideSelf` `?speakingOnly` `?noName` `?size=48` `?align=right` `?layout=row`

## À savoir
- Ne montre jamais `config.json` ni `token.json` en stream et ne les partage pas : ils contiennent tes clés privées.
- Pour donner l'appli à quelqu'un, envoie-lui **seulement** `Voice Overlay.exe`. Il créera sa propre appli Discord depuis la page de réglages.
- En cas de souci, le fichier `voice-overlay.log` (à côté du .exe) dit ce qui s'est passé.
- Sans le .exe, on peut aussi lancer `start.bat` (il faut alors Node.js).

## Crédits
Polices fournies sous licence libre SIL Open Font License 1.1 (Inter, Poppins, Montserrat, Rubik, Fredoka, Oswald, Barlow Condensed, Rajdhani, Orbitron, Bebas Neue, Russo One, Bangers). Moteur : Node.js.
