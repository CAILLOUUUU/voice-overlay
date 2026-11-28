# VOICE OVERLAY - CAILLOU FEAT CLAUDE

🇬🇧 [English version](README.en.md)

Un overlay pour OBS qui affiche qui est dans **ton** salon vocal Discord, et qui te suit tout seul quand tu changes de salon.

![Réglages avec aperçu en direct](docs/settings.png)

- Montre qui parle, qui a coupé son micro ou le son
- Change de salon avec toi, et disparaît quand tu n'es dans aucun
- Renomme, réordonne ou masque des personnes
- Style entièrement réglable, avec des styles prêts à l'emploi
- Rôles League of Legends affichés et triés (Top, Jgl, Mid, ADC, Sup)
- Plusieurs profils en même temps, un par scène OBS
- En français et en anglais

## Installation (Windows)

1. Télécharge le dernier `Voice-Overlay-<version>.exe` dans [Releases](../../releases/latest).
2. Ouvre Discord (l'appli, pas le site), puis lance le `.exe`.
   Si Windows affiche « Windows a protégé votre ordinateur », clique sur **Informations complémentaires**, puis **Exécuter quand même**.
3. Suis l'assistant (environ 3 minutes, une seule fois), puis clique sur **Autoriser** dans Discord.

L'appli tourne ensuite en arrière-plan, avec une icône près de l'horloge. Elle se met à jour en un clic.

## Dans OBS

Ajoute une source **Navigateur** avec le lien affiché dans les réglages (onglet « Profil et OBS »), en 400 × 600 pour commencer. Le fond est transparent.

## Inhouses DPM

Avec l'extension Chrome du dossier [`extension`](extension), l'overlay récupère le rôle et l'équipe de chaque joueur de ton lobby [inhouse.dpm.lol](https://inhouse.dpm.lol), et relie tout seul chaque compte Discord à son Riot ID.

La détection n'est pas parfaite : si un rôle est faux, choisis le bon à la main dans l'onglet « Personnes ».

## Overlay en jeu

L'onglet « En jeu » affiche ton vocal par-dessus League pendant tes parties. Il est invisible en stream. Il faut que League soit en mode « Sans bordure » ou « Fenêtré ». Ctrl+Maj+O pour le déplacer.

## Bon à savoir

- Tout reste sur ton PC. L'appli ne se connecte qu'à Discord et à GitHub (pour les mises à jour).
- Elle utilise une fonction de Discord encore en bêta : si Discord la change, l'overlay peut cesser de marcher.
- En cas de souci : onglet « Application » → **Copier le diagnostic**, et envoie-le avec ta question.

## Crédits

Fait par Caillou, avec Claude. Version Rust par tbrowang. [Licence MIT](LICENSE) · [Licences des composants](LICENCES-TIERS.txt)
