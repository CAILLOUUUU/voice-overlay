// Tourne dans la page DPM. Lit les réponses « lobby » que la page reçoit déjà :
// aucune requête en plus vers DPM, et rien d'autre n'est lu (ni cookies, ni compte).
(() => {
  const LOBBY = /\/api\/v1\/lobby\/[\w-]+$/;
  const send = data => window.postMessage({ source: 'voice-overlay-dpm', lobby: data }, location.origin);
  const pick = j => j && Array.isArray(j.players) ? {
    matchId: j.matchId, state: j.state,
    players: j.players.map(p => ({ gameName: p.gameName, tagLine: p.tagLine, side: p.side, lane: p.lane, chosenRole: p.chosenRole, inVoice: p.inVoice })),
  } : null;
  const origFetch = window.fetch;
  window.fetch = async function (input) {
    const res = await origFetch.apply(this, arguments);
    try {
      const url = new URL(typeof input === 'string' ? input : input.url, location.href);
      if (url.origin === location.origin && LOBBY.test(url.pathname)) {
        res.clone().json().then(j => { const d = pick(j); if (d) send(d); }).catch(() => {});
      }
    } catch {}
    return res;
  };
})();
