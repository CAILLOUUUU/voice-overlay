// Fait passer les lobbies de la page vers l'extension (la page ne peut pas parler à l'appli directement).
window.addEventListener('message', e => {
  if (e.source !== window || !e.data || e.data.source !== 'voice-overlay-dpm') return;
  chrome.runtime.sendMessage({ type: 'lobby', lobby: e.data.lobby }).catch(() => {});
});
