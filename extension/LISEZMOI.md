# Voice Overlay – pont DPM

Petite extension Chrome qui envoie le lobby de [inhouse.dpm.lol](https://inhouse.dpm.lol) à l'appli Voice Overlay ouverte sur ton PC.
Avec elle, l'overlay connaît le rôle et l'équipe de chaque joueur, et apprend tout seul qui est qui sur Discord.

## Installer (mode développeur)
1. Ouvre `chrome://extensions` et active le **mode développeur** (en haut à droite).
2. Clique sur **Charger l'extension non empaquetée** et choisis ce dossier `extension`.
3. Garde l'appli Voice Overlay ouverte pendant tes inhouses.

## Ce qu'elle lit, et rien d'autre
- Seulement sur `inhouse.dpm.lol`, et seulement les réponses « lobby » que ta page reçoit déjà : Riot ID, équipe, rôle et « dans le vocal : oui/non » de chaque joueur.
- Aucune requête en plus vers DPM, aucun cookie, aucun mot de passe.
- Tout est envoyé uniquement à l'appli sur ton PC (`127.0.0.1`).

## Pour une autre extension (ex. celle de Marso)
Il suffit d'envoyer le lobby à l'appli :

```
POST http://127.0.0.1:47474/api/dpm
Content-Type: application/json

{ "matchId": "...", "state": "gathering", "you": { "side": "red" },
  "players": [ { "gameName": "...", "tagLine": "...", "side": "blue|red", "lane": "top|jungle|mid|adc|support", "inVoice": true } ] }
```

L'appli accepte cet appel uniquement depuis une extension Chrome (`chrome-extension://…`) ou depuis ses propres pages.
Pour le voyant « Pont DPM connecté », l'extension appelle aussi `GET http://127.0.0.1:47474/api/dpm/ping` environ une fois par minute.
