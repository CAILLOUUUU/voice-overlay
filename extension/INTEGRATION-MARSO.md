# Intégrer le pont Voice Overlay dans une extension existante

But : que les gens qui ont déjà ton extension DPM n'aient qu'à installer l'appli Voice Overlay.
Ton extension envoie le lobby DPM à l'appli, qui tourne sur le PC de l'utilisateur.

**Aucune nouvelle permission n'est nécessaire** : l'appli accepte les appels venant de n'importe quelle extension Chrome (`chrome-extension://…`) et refuse les sites web. Tu n'as rien à ajouter dans `host_permissions`, donc pas de nouvel avertissement pour tes utilisateurs.

## 1. Ce qu'il faut envoyer

À chaque fois que le lobby change (pastille qui passe au vert, rôles, état), envoie ceci depuis le **service worker** de ton extension (pas depuis un content script : Chrome bloque les appels d'une page web vers `127.0.0.1`).

```js
fetch('http://127.0.0.1:47474/api/dpm', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    matchId: lobby.matchId,          // identifiant du match DPM
    state: lobby.state,              // "gathering", "launching", "playing"…
    you: { side: lobby.you.side },   // "blue" ou "red" : l'équipe de l'utilisateur
    players: lobby.players.map(p => ({
      gameName: p.gameName,          // Riot ID, partie avant le #
      tagLine: p.tagLine,            // Riot ID, partie après le #
      side: p.side,                  // "blue" ou "red"
      lane: p.lane,                  // "top", "jungle", "mid", "adc", "support"
      inVoice: p.inVoice,            // true quand la pastille est verte
    })),
  }),
}).catch(() => {});                  // appli fermée : on ignore
```

C'est exactement le contenu de la réponse `GET /api/v1/lobby/<id>` que la page DPM reçoit déjà, réduit à ces champs.

**Le plus important : envoyer dès que `inVoice` change.** L'appli relie chaque personne Discord à son Riot ID en comparant l'instant où elle rejoint le vocal Discord à l'instant où sa pastille passe au vert. Plus l'envoi est rapide après le changement, mieux ça marche. Le plus simple : intercepter les réponses `/api/v1/lobby/<id>` que la page fait déjà (aucune requête en plus vers DPM), comme le fait `page.js` dans ce dossier.

## 2. Signal de vie (recommandé)

Dans les réglages, l'appli affiche un voyant « Pont DPM connecté ». Pour qu'il reste vert même quand aucune page DPM n'est ouverte, appelle `/api/dpm/ping` depuis le service worker environ une fois par minute (sinon le voyant passe au gris 2 à 3 minutes après le dernier lobby reçu) :

```js
// manifest.json : "permissions": ["alarms"] (pas d'avertissement pour les utilisateurs)
const ping = () => fetch('http://127.0.0.1:47474/api/dpm/ping').catch(() => {});
const start = () => { chrome.alarms.create('vo-ping', { periodInMinutes: 1 }); ping(); };
chrome.runtime.onInstalled.addListener(start);
chrome.runtime.onStartup.addListener(start);
chrome.alarms.onAlarm.addListener(a => { if (a.name === 'vo-ping') ping(); });
```

## 3. Savoir si l'appli est lancée (facultatif)

```js
const r = await fetch('http://127.0.0.1:47474/api/dpm/ping').catch(() => null);
// r && r.ok  → { app: "voice-overlay", version: "1.2.0" }
// sinon      → appli fermée ou pas installée : tu peux afficher un lien de téléchargement
```

Lien de téléchargement : https://github.com/CAILLOUUUU/voice-overlay/releases/latest

## 3. Points à vérifier pour le Chrome Web Store

- Mets à jour ta politique de confidentialité / la description : le lobby DPM (Riot ID, équipe, rôle, présence vocale) est envoyé **uniquement à une appli locale sur le PC de l'utilisateur** (`127.0.0.1`), jamais sur Internet.
- Idéalement, mets l'envoi derrière une option « Envoyer à Voice Overlay » dans tes réglages, activée par défaut ou non selon ton choix.
- Le port est `47474` par défaut. Un utilisateur peut le changer dans `config.json` de l'appli ; si tu veux le gérer, ajoute un champ « port » dans tes options.

## Référence

Les fichiers `page.js`, `relay.js` et `background.js` de ce dossier font exactement ça et peuvent être repris tels quels (licence MIT).
