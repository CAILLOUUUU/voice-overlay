// Envoie le lobby à l'appli Voice Overlay qui tourne sur ce PC.
const APP = 'http://127.0.0.1:47474';

chrome.runtime.onMessage.addListener(msg => {
  if (!msg || msg.type !== 'lobby' || !msg.lobby) return;
  fetch(APP + '/api/dpm', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(msg.lobby),
  }).catch(() => {}); // appli fermée : on ignore
});

// Signal de vie chaque minute, même sans page DPM ouverte :
// l'appli affiche alors « Pont DPM connecté » dans ses réglages.
const ping = () => fetch(APP + '/api/dpm/ping').catch(() => {});
const start = () => { chrome.alarms.create('vo-ping', { periodInMinutes: 1 }); ping(); };
chrome.runtime.onInstalled.addListener(start);
chrome.runtime.onStartup.addListener(start);
chrome.alarms.onAlarm.addListener(a => { if (a.name === 'vo-ping') ping(); });
