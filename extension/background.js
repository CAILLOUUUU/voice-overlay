// Envoie le lobby à l'appli Voice Overlay qui tourne sur ce PC.
chrome.runtime.onMessage.addListener(msg => {
  if (!msg || msg.type !== 'lobby' || !msg.lobby) return;
  fetch('http://127.0.0.1:47474/api/dpm', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(msg.lobby),
  }).catch(() => {}); // appli fermée : on ignore
});
