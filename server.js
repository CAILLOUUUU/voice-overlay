// VOICE OVERLAY - CAILLOU FEAT CLAUDE
// Suit automatiquement ton salon vocal Discord et affiche qui est dedans
// dans une source navigateur OBS. Aucune dépendance : Node.js 20+ suffit.
//
//   node server.js          -> mode normal (Discord desktop doit être ouvert)
//   node server.js --open   -> pareil + ouvre la page de réglages
//   node server.js --demo   -> faux membres (sans Discord)
// Ce même fichier est aussi embarqué tel quel dans "Voice Overlay.exe".

const net = require('net');
const http = require('http');
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { exec, execFile, spawn } = require('child_process');

const APP_NAME = 'VOICE OVERLAY - CAILLOU FEAT CLAUDE';
const WIN = process.platform === 'win32';
const DEMO = process.argv.includes('--demo');

// ---------- Mode .exe (Node "single executable application") ----------
let sea = null;
try { sea = require('node:sea'); if (!sea.isSea()) sea = null; } catch { sea = null; }

function asset(name) {
  return sea ? Buffer.from(sea.getAsset(name)) : fs.readFileSync(path.join(__dirname, name));
}

// Dossier des données : à côté du programme, sinon %APPDATA% si on ne peut pas y écrire.
function pickDataDir() {
  const base = sea ? path.dirname(process.execPath) : __dirname;
  try {
    const probe = path.join(base, '.write-test');
    fs.writeFileSync(probe, '');
    fs.unlinkSync(probe);
    return base;
  } catch {
    const alt = path.join(process.env.APPDATA || os.homedir(), 'VoiceOverlayCaillou');
    fs.mkdirSync(alt, { recursive: true });
    return alt;
  }
}
const DIR = pickDataDir();
const CONFIG_PATH = path.join(DIR, 'config.json');
const TOKEN_PATH = path.join(DIR, 'token.json');
const SETTINGS_PATH = path.join(DIR, 'settings.json');
const LOG_PATH = path.join(DIR, 'voice-overlay.log');

// Le .exe n'a pas de fenêtre : on écrit le journal dans un fichier.
if (sea) {
  try { if (fs.statSync(LOG_PATH).size > 300000) fs.writeFileSync(LOG_PATH, ''); } catch {}
  const write = (...a) => {
    try { fs.appendFileSync(LOG_PATH, `[${new Date().toLocaleTimeString()}] ${a.join(' ')}\n`); } catch {}
  };
  console.log = write;
  console.error = write;
  process.on('uncaughtException', e => write('ERREUR', e && e.stack || e));
  process.on('unhandledRejection', e => write('ERREUR', e && e.stack || e));
}

function readJson(p) {
  try { return JSON.parse(fs.readFileSync(p, 'utf8')); } catch { return null; }
}

function openBrowser(url) {
  if (WIN) exec(`start "" "${url}"`, { windowsHide: true });
}

// ---------- Polices fournies + fichiers ajoutés par l'utilisateur ----------
// Polices libres (licence OFL) embarquées dans l'appli : [nom, graisses disponibles]
const BUILTIN_FONTS = [
  ['Inter', [500, 700]], ['Poppins', [500, 700]], ['Montserrat', [500, 700]], ['Rubik', [500, 700]],
  ['Fredoka', [500, 700]], ['Oswald', [500, 700]], ['Barlow Condensed', [500, 700]], ['Rajdhani', [500, 700]],
  ['Orbitron', [500, 700]], ['Bebas Neue', [400]], ['Russo One', [400]], ['Bangers', [400]],
];
const slug = n => n.toLowerCase().replace(/ /g, '-');
const USER_DIRS = { font: path.join(DIR, 'polices') };
const USER_EXT = { font: ['.ttf', '.otf', '.woff', '.woff2'] };
const USER_MAX = { font: 10e6 };
const MIME = { '.ttf': 'font/ttf', '.otf': 'font/otf', '.woff': 'font/woff', '.woff2': 'font/woff2' };
let assetsVersion = Date.now();

function listUser(kind) {
  try {
    return fs.readdirSync(USER_DIRS[kind]).filter(f => USER_EXT[kind].includes(path.extname(f).toLowerCase())).sort();
  } catch { return []; }
}
// Nom de fichier sûr (pas de sous-dossier, extension autorisée), sinon null.
function safeName(kind, raw) {
  const base = path.basename(String(raw || '').replace(/\\/g, '/')).replace(/[^\w .()\-]/g, '_').slice(0, 80);
  const ext = path.extname(base).toLowerCase();
  return USER_EXT[kind].includes(ext) && path.basename(base, ext).trim() ? base : null;
}
const fontFamily = file => path.basename(file, path.extname(file));

function fontsCss() {
  let css = '';
  for (const [name, weights] of BUILTIN_FONTS) {
    for (const w of weights) {
      css += `@font-face{font-family:"${name}";font-weight:${weights.length === 1 ? '100 900' : w};font-display:swap;src:url("/webfonts/${slug(name)}-${w}.woff2") format("woff2")}\n`;
    }
  }
  for (const f of listUser('font')) {
    css += `@font-face{font-family:${JSON.stringify(fontFamily(f))};font-weight:100 900;font-display:swap;src:url("/polices/${encodeURIComponent(f)}")}\n`;
  }
  return css;
}

// ---------- Config (identifiants Discord) ----------
const config = { port: 3000, redirectUri: 'http://localhost', clientId: '', clientSecret: '' };
Object.assign(config, readJson(CONFIG_PATH) || {});
if (/^COLLE_/.test(config.clientId)) config.clientId = '';
if (/^COLLE_/.test(config.clientSecret)) config.clientSecret = '';
const hasConfig = () => !!(config.clientId && config.clientSecret);
const BASE_URL = `http://localhost:${config.port}`;

// ---------- Réglages de l'overlay (modifiables depuis /settings) ----------
const DEFAULTS = {
  showTitle: true,
  titleText: '{salon}',
  layout: 'column',      // column | row
  align: 'left',         // left | right
  size: 40,
  gap: 6,
  shape: 'circle',       // circle | rounded | square
  showNames: true,
  showIcons: true,
  hideSelf: false,
  speakingOnly: false,
  dimSilent: false,
  effect: 'glow',        // ring | glow | bounce
  animate: true,
  speakColor: '#3ba55d',
  accentColor: '#8b6cff',
  textColor: '#ffffff',
  bgColor: '#121318',
  bgOpacity: 86,
  font: 'Inter',
  bold: true,
  customCss: '',
};
const settings = { ...DEFAULTS };

function applySettings(input) {
  if (!input || typeof input !== 'object') return;
  for (const k of Object.keys(DEFAULTS)) {
    if (!(k in input)) continue;
    const d = DEFAULTS[k];
    const v = input[k];
    if (typeof d === 'boolean') settings[k] = !!v;
    else if (typeof d === 'number') { const n = Number(v); if (Number.isFinite(n)) settings[k] = n; }
    else settings[k] = String(v).slice(0, k === 'customCss' ? 20000 : 200);
  }
}
applySettings(readJson(SETTINGS_PATH));

// ---------- État partagé ----------
const state = {
  status: DEMO ? 'demo' : 'starting',  // setup | no-discord | auth | connected | error | demo
  statusMsg: '',
  user: null,
  channel: null,           // { id, name, guildId }
  selfId: null,
  members: new Map(),      // id -> { id, name, avatar, mute, deaf }   (ordre d'arrivée)
  speaking: new Set(),
  // Réglages par personne : valables tant que la personne est dans ton vocal.
  custom: new Map(),       // id -> { name?, hidden? }
  order: [],               // ordre personnalisé (ids)
};

function orderedMembers() {
  const pos = new Map(state.order.map((id, i) => [id, i]));
  return [...state.members.values()]
    .map((m, i) => ({ m, k: pos.has(m.id) ? pos.get(m.id) : 1e6 + i }))
    .sort((a, b) => a.k - b.k)
    .map(x => x.m);
}

function snapshot() {
  return {
    status: state.status,
    statusMsg: state.statusMsg,
    user: state.user,
    settings,
    assetsVersion,
    channel: state.channel,
    selfId: state.selfId,
    members: orderedMembers().map(m => {
      const c = state.custom.get(m.id) || {};
      return { ...m, realName: m.name, name: c.name || m.name, hidden: !!c.hidden, speaking: state.speaking.has(m.id) };
    }),
  };
}

// Oublie les réglages des personnes qui ne sont plus dans le vocal.
function prunePeople() {
  for (const id of state.custom.keys()) if (!state.members.has(id)) state.custom.delete(id);
  state.order = state.order.filter(id => state.members.has(id));
}

function resetState() {
  state.channel = null;
  state.members.clear();
  state.speaking.clear();
}

function setStatus(status, msg = '') {
  if (state.status === status && state.statusMsg === msg) return;
  state.status = status;
  state.statusMsg = msg;
  if (msg) console.log(msg);
  broadcast();
}

// ---------- Lancement avec Windows ----------
const RUN_KEY = 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run';
const RUN_NAME = 'VoiceOverlayCaillou';
const canAutostart = !!(sea && WIN);

function getAutostart() {
  return new Promise(r => execFile('reg', ['query', RUN_KEY, '/v', RUN_NAME], { windowsHide: true }, e => r(!e)));
}
function setAutostart(on) {
  const args = on
    ? ['add', RUN_KEY, '/v', RUN_NAME, '/t', 'REG_SZ', '/d', `"${process.execPath}" --silent`, '/f']
    : ['delete', RUN_KEY, '/v', RUN_NAME, '/f'];
  return new Promise(r => execFile('reg', args, { windowsHide: true }, e => r(!e)));
}

// ---------- Icône à côté de l'horloge (Windows) ----------
function startTray() {
  try {
    const tmp = path.join(os.tmpdir(), 'VoiceOverlayCaillou');
    fs.mkdirSync(tmp, { recursive: true });
    const ico = path.join(tmp, 'icon.ico');
    const ps1 = path.join(tmp, 'tray.ps1');
    fs.writeFileSync(ico, asset('icon.ico'));
    const script = `param([int]$ParentPid, [string]$Url, [string]$Icon)
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$ni = New-Object System.Windows.Forms.NotifyIcon
$ni.Icon = New-Object System.Drawing.Icon($Icon)
$ni.Text = 'Voice Overlay - Caillou feat Claude'
$ni.Visible = $true
$menu = New-Object System.Windows.Forms.ContextMenuStrip
$open = $menu.Items.Add('Ouvrir les réglages')
$open.add_Click({ Start-Process $Url })
$quit = $menu.Items.Add('Quitter')
$quit.add_Click({
  try { Stop-Process -Id $ParentPid -Force } catch {}
  $ni.Visible = $false
  [System.Windows.Forms.Application]::Exit()
})
$ni.ContextMenuStrip = $menu
$ni.add_MouseClick({ param($s, $e) if ($e.Button -eq [System.Windows.Forms.MouseButtons]::Left) { Start-Process $Url } })
$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 2000
$timer.add_Tick({
  if (-not (Get-Process -Id $ParentPid -ErrorAction SilentlyContinue)) {
    $ni.Visible = $false
    [System.Windows.Forms.Application]::Exit()
  }
})
$timer.Start()
[System.Windows.Forms.Application]::Run()
`;
    fs.writeFileSync(ps1, '﻿' + script, 'utf8'); // BOM : sinon les accents cassent dans PowerShell 5
    const child = spawn('powershell.exe', [
      '-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', ps1,
      '-ParentPid', String(process.pid), '-Url', `${BASE_URL}/settings`, '-Icon', ico,
    ], { windowsHide: true, stdio: 'ignore' });
    child.on('error', e => console.error('Icône de notification indisponible :', e.message));
  } catch (e) {
    console.error('Icône de notification indisponible :', e.message);
  }
}

// ---------- Serveur HTTP + flux temps réel (SSE) ----------
const clients = new Set();

function broadcast() {
  const msg = `data: ${JSON.stringify(snapshot())}\n\n`;
  for (const res of clients) res.write(msg);
}

// Seules les pages servies par ce serveur ont le droit de modifier les réglages.
function isLocalRequest(req) {
  const ok = [`localhost:${config.port}`, `127.0.0.1:${config.port}`];
  if (!ok.includes(req.headers.host)) return false;
  const o = req.headers.origin;
  return !o || ok.some(h => o === `http://${h}`);
}

function readRaw(req, max) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    req.on('data', c => { size += c.length; if (size > max) { reject(new Error('Fichier trop gros.')); req.destroy(); } else chunks.push(c); });
    req.on('end', () => resolve(Buffer.concat(chunks)));
    req.on('error', reject);
  });
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    let data = '';
    req.on('data', c => { data += c; if (data.length > 200000) { reject(new Error('trop gros')); req.destroy(); } });
    req.on('end', () => { try { resolve(JSON.parse(data || '{}')); } catch (e) { reject(e); } });
    req.on('error', reject);
  });
}

function sendAsset(res, name, type) {
  try {
    const body = asset(name);
    res.writeHead(200, { 'Content-Type': type, 'Cache-Control': 'no-cache' });
    res.end(body);
  } catch {
    res.writeHead(404);
    res.end();
  }
}

function sendJson(res, code, obj) {
  res.writeHead(code, { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-cache' });
  res.end(JSON.stringify(obj));
}

let restartRequested = false;

async function handleApi(url, req, res) {
  const post = req.method === 'POST';
  if (url === '/api/state' && !post) {
    return sendJson(res, 200, {
      app: APP_NAME, settings, defaults: DEFAULTS, clientId: config.clientId, hasConfig: hasConfig(), port: config.port,
      canAutostart, autostart: canAutostart ? await getAutostart() : false, isExe: !!sea, dataDir: DIR,
    });
  }
  if (url === '/api/files' && !post) {
    return sendJson(res, 200, {
      builtinFonts: BUILTIN_FONTS.map(f => f[0]),
      fonts: listUser('font').map(file => ({ file, family: fontFamily(file) })),
    });
  }
  if (!post) return sendJson(res, 404, { error: 'inconnu' });

  // Import d'une police : le fichier est envoyé tel quel.
  if (url === '/api/upload') {
    const q = new URL(req.url, BASE_URL).searchParams;
    const kind = q.get('kind');
    if (!USER_DIRS[kind]) return sendJson(res, 400, { error: 'Type inconnu.' });
    const name = safeName(kind, q.get('name'));
    if (!name) return sendJson(res, 400, { error: `Format non accepté. Formats possibles : ${USER_EXT[kind].join(', ')}` });
    const data = await readRaw(req, USER_MAX[kind]);
    if (!data.length) return sendJson(res, 400, { error: 'Fichier vide.' });
    fs.mkdirSync(USER_DIRS[kind], { recursive: true });
    fs.writeFileSync(path.join(USER_DIRS[kind], name), data);
    assetsVersion = Date.now();
    broadcast();
    return sendJson(res, 200, { ok: true, file: name, family: fontFamily(name) });
  }

  const b = await readBody(req);

  switch (url) {
    case '/api/settings':
      applySettings(b);
      fs.writeFileSync(SETTINGS_PATH, JSON.stringify(settings, null, 2));
      broadcast();
      return sendJson(res, 200, { ok: true });

    case '/api/config': {
      const id = String(b.clientId || '').trim();
      const secret = String(b.clientSecret || '').trim() || (id === config.clientId ? config.clientSecret : '');
      if (!/^\d{15,22}$/.test(id)) return sendJson(res, 400, { error: 'Le Client ID doit être une suite de chiffres.' });
      if (secret.length < 10) return sendJson(res, 400, { error: 'Le Client Secret a l\'air incomplet.' });
      config.clientId = id;
      config.clientSecret = secret;
      fs.writeFileSync(CONFIG_PATH, JSON.stringify(config, null, 2));
      try { fs.unlinkSync(TOKEN_PATH); } catch {}
      restartRequested = true;
      if (rpc && rpc.sock) rpc.sock.destroy();
      return sendJson(res, 200, { ok: true });
    }

    // --- Réglages par personne (oubliés quand la personne quitte le vocal) ---
    case '/api/person': {
      const id = String(b.id || '');
      if (!state.members.has(id)) return sendJson(res, 404, { error: 'Cette personne n\'est plus dans le vocal.' });
      const c = { ...(state.custom.get(id) || {}) };
      if ('name' in b) c.name = String(b.name || '').trim().slice(0, 40);
      if ('hidden' in b) c.hidden = !!b.hidden;
      if (c.name || c.hidden) state.custom.set(id, c); else state.custom.delete(id);
      broadcast();
      return sendJson(res, 200, { ok: true });
    }
    case '/api/order':
      state.order = (Array.isArray(b.order) ? b.order : []).map(String).filter(id => state.members.has(id));
      broadcast();
      return sendJson(res, 200, { ok: true });
    case '/api/people-reset':
      state.custom.clear();
      state.order = [];
      broadcast();
      return sendJson(res, 200, { ok: true });

    case '/api/delete-file': {
      const name = USER_DIRS[b.kind] && safeName(b.kind, b.file);
      if (!name) return sendJson(res, 400, { error: 'Fichier inconnu.' });
      try { fs.unlinkSync(path.join(USER_DIRS[b.kind], name)); } catch {}
      assetsVersion = Date.now();
      broadcast();
      return sendJson(res, 200, { ok: true });
    }

    case '/api/autostart':
      if (!canAutostart) return sendJson(res, 400, { error: 'Disponible uniquement avec le .exe sous Windows.' });
      await setAutostart(!!b.enabled);
      return sendJson(res, 200, { ok: true, autostart: await getAutostart() });
    case '/api/quit':
      sendJson(res, 200, { ok: true });
      console.log('Fermeture demandée depuis la page de réglages.');
      return setTimeout(() => process.exit(0), 300);
  }
  return sendJson(res, 404, { error: 'inconnu' });
}

http.createServer((req, res) => {
  const url = req.url.split('?')[0];

  if (url === '/events') {
    res.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache', Connection: 'keep-alive' });
    res.write(`data: ${JSON.stringify(snapshot())}\n\n`);
    clients.add(res);
    req.on('close', () => clients.delete(res));
    return;
  }
  if (url === '/') return sendAsset(res, 'overlay.html', 'text/html; charset=utf-8');
  if (url === '/settings') return sendAsset(res, 'settings.html', 'text/html; charset=utf-8');
  if (url === '/icon.png' || url === '/favicon.ico') return sendAsset(res, 'icon.png', 'image/png');

  if (url === '/fonts.css') {
    res.writeHead(200, { 'Content-Type': 'text/css; charset=utf-8', 'Cache-Control': 'no-cache' });
    return res.end(fontsCss());
  }
  let m;
  if ((m = /^\/webfonts\/([\w-]+\.woff2)$/.exec(url))) return sendAsset(res, 'webfonts/' + m[1], 'font/woff2');
  if ((m = /^\/polices\/([^/]+)$/.exec(url))) {
    const kind = 'font';
    let name = null;
    try { name = safeName(kind, decodeURIComponent(m[1])); } catch {}
    if (!name) { res.writeHead(404); return res.end(); }
    return fs.readFile(path.join(USER_DIRS[kind], name), (err, data) => {
      if (err) { res.writeHead(404); return res.end(); }
      res.writeHead(200, { 'Content-Type': MIME[path.extname(name).toLowerCase()], 'Cache-Control': 'no-cache' });
      res.end(data);
    });
  }

  if (url.startsWith('/api/')) {
    if (!isLocalRequest(req)) return sendJson(res, 403, { error: 'interdit' });
    return handleApi(url, req, res).catch(e => sendJson(res, 400, { error: e.message }));
  }
  res.writeHead(404);
  res.end();
}).listen(config.port, '127.0.0.1', () => {
  console.log(`=== ${APP_NAME} ===`);
  console.log(`Réglages : ${BASE_URL}/settings`);
  console.log(`Overlay  : ${BASE_URL}/   (à mettre dans une source navigateur OBS)`);
  const silent = process.argv.includes('--silent');
  if (process.argv.includes('--open') || (sea && !silent)) openBrowser(`${BASE_URL}/settings`);
  if (sea && WIN) startTray();
}).on('error', e => {
  if (e.code === 'EADDRINUSE') {
    // Déjà lancé : on ouvre simplement la page de réglages de l'instance en cours.
    console.error(`Le port ${config.port} est déjà utilisé : l'overlay tourne déjà.`);
    if (sea) openBrowser(`${BASE_URL}/settings`);
    return setTimeout(() => process.exit(0), 800);
  }
  console.error(e.message);
  process.exit(1);
});

// garde les connexions SSE vivantes
setInterval(() => { for (const res of clients) res.write(': ping\n\n'); }, 20000);

// ---------- Conversion d'un "voice state" Discord ----------
function toMember(vs, guildId) {
  const u = vs.user;
  const v = vs.voice_state || {};
  // Avatar de serveur si Discord le fournit, sinon avatar du compte, sinon avatar par défaut.
  const guildAvatar = (vs.member && vs.member.avatar) || vs.guild_avatar || u.guild_avatar || null;
  let avatar;
  if (guildAvatar && guildId) avatar = `https://cdn.discordapp.com/guilds/${guildId}/users/${u.id}/avatars/${guildAvatar}.png?size=128`;
  else if (u.avatar) avatar = `https://cdn.discordapp.com/avatars/${u.id}/${u.avatar}.png?size=128`;
  else avatar = `https://cdn.discordapp.com/embed/avatars/${Number((BigInt(u.id) >> 22n) % 6n)}.png`;
  return {
    id: u.id,
    name: vs.nick || u.global_name || u.username,
    avatar,
    mute: !!(v.mute || v.self_mute),
    deaf: !!(v.deaf || v.self_deaf),
  };
}

// ---------- Client RPC Discord (IPC local) ----------
const OP = { HANDSHAKE: 0, FRAME: 1, CLOSE: 2, PING: 3, PONG: 4 };

function pipePath(i) {
  if (WIN) return `\\\\?\\pipe\\discord-ipc-${i}`;
  const base = (process.env.XDG_RUNTIME_DIR || process.env.TMPDIR || process.env.TMP || process.env.TEMP || '/tmp').replace(/\/$/, '');
  return `${base}/discord-ipc-${i}`;
}

class DiscordRPC {
  constructor(clientId) {
    this.clientId = clientId;
    this.pending = new Map();
    this.onEvent = () => {};
    this.buf = Buffer.alloc(0);
    this.closed = new Promise(r => (this._resolveClosed = r));
  }

  connect() {
    return new Promise((resolve, reject) => {
      const attempt = i => {
        if (i > 9) { const e = new Error('Discord introuvable'); e.code = 'NO_DISCORD'; return reject(e); }
        const sock = net.createConnection(pipePath(i));
        sock.once('error', () => attempt(i + 1));
        sock.once('connect', () => {
          sock.removeAllListeners('error');
          this.sock = sock;
          this._ready = resolve;
          this._fail = reject;
          sock.on('data', d => this._onData(d));
          sock.on('error', e => console.error('[RPC] erreur socket :', e.message));
          sock.on('close', () => this._onClose());
          this._send(OP.HANDSHAKE, { v: 1, client_id: this.clientId });
        });
      };
      attempt(0);
    });
  }

  _send(op, payload) {
    const json = Buffer.from(JSON.stringify(payload), 'utf8');
    const header = Buffer.alloc(8);
    header.writeInt32LE(op, 0);
    header.writeInt32LE(json.length, 4);
    this.sock.write(Buffer.concat([header, json]));
  }

  _onData(chunk) {
    this.buf = Buffer.concat([this.buf, chunk]);
    while (this.buf.length >= 8) {
      const op = this.buf.readInt32LE(0);
      const len = this.buf.readInt32LE(4);
      if (this.buf.length < 8 + len) break;
      const msg = JSON.parse(this.buf.subarray(8, 8 + len).toString('utf8'));
      this.buf = this.buf.subarray(8 + len);
      this._handle(op, msg);
    }
  }

  _handle(op, msg) {
    if (op === OP.PING) return this._send(OP.PONG, msg);
    if (op === OP.CLOSE) {
      this.closeReason = msg.message || JSON.stringify(msg);
      return this.sock.destroy();
    }
    if (msg.cmd === 'DISPATCH' && msg.evt === 'READY') {
      if (this._ready) { this._ready(); this._ready = null; this._fail = null; }
      return;
    }
    if (msg.nonce && this.pending.has(msg.nonce)) {
      const { resolve, reject } = this.pending.get(msg.nonce);
      this.pending.delete(msg.nonce);
      if (msg.evt === 'ERROR') reject(new Error(`${msg.cmd} : ${msg.data && msg.data.message}`));
      else resolve(msg.data);
      return;
    }
    if (msg.cmd === 'DISPATCH') this.onEvent(msg.evt, msg.data);
  }

  _onClose() {
    const err = new Error(this.closeReason ? `Discord a refusé la connexion : ${this.closeReason}` : 'Connexion Discord fermée');
    if (this._fail) { this._fail(err); this._fail = null; }
    for (const { reject } of this.pending.values()) reject(err);
    this.pending.clear();
    this._resolveClosed();
  }

  request(cmd, args = {}, evt) {
    return new Promise((resolve, reject) => {
      const nonce = crypto.randomUUID();
      this.pending.set(nonce, { resolve, reject });
      const payload = { cmd, args, nonce };
      if (evt) payload.evt = evt;
      this._send(OP.FRAME, payload);
    });
  }
}

// ---------- Authentification (une seule fois, puis token sauvegardé) ----------
async function exchangeToken(params) {
  const body = new URLSearchParams({ client_id: config.clientId, client_secret: config.clientSecret, ...params });
  const r = await fetch('https://discord.com/api/oauth2/token', {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body,
  });
  const j = await r.json();
  if (!r.ok) throw new Error(`Discord a refusé les identifiants (${j.error || r.status}). Vérifie le Client ID, le Client Secret et la redirection http://localhost.`);
  fs.writeFileSync(TOKEN_PATH, JSON.stringify(j, null, 2));
  return j;
}

async function authenticate(rpc) {
  let tok = readJson(TOKEN_PATH);
  if (tok) {
    try { return await rpc.request('AUTHENTICATE', { access_token: tok.access_token }); } catch {}
    if (tok.refresh_token) {
      try {
        tok = await exchangeToken({ grant_type: 'refresh_token', refresh_token: tok.refresh_token });
        return await rpc.request('AUTHENTICATE', { access_token: tok.access_token });
      } catch {}
    }
  }
  setStatus('auth', 'Une fenêtre s\'est ouverte dans Discord : clique sur "Autoriser".');
  const { code } = await rpc.request('AUTHORIZE', { client_id: config.clientId, scopes: ['rpc', 'identify'] });
  tok = await exchangeToken({ grant_type: 'authorization_code', code, redirect_uri: config.redirectUri });
  return rpc.request('AUTHENTICATE', { access_token: tok.access_token });
}

// ---------- Suivi du salon vocal ----------
const CHANNEL_EVENTS = ['VOICE_STATE_CREATE', 'VOICE_STATE_UPDATE', 'VOICE_STATE_DELETE', 'SPEAKING_START', 'SPEAKING_STOP'];
let rpc = null;
let currentChannel = null;
let followQueue = Promise.resolve();

function queueFollow(channelId) {
  followQueue = followQueue.then(() => follow(channelId)).catch(e => console.error('[Suivi]', e.message));
}

async function follow(channelId) {
  if (channelId === currentChannel) return;
  if (currentChannel) {
    for (const evt of CHANNEL_EVENTS) rpc.request('UNSUBSCRIBE', { channel_id: currentChannel }, evt).catch(() => {});
  }
  currentChannel = channelId;
  resetState();

  if (!channelId) {
    console.log('Tu n\'es dans aucun vocal.');
    prunePeople();
    return broadcast();
  }

  const ch = await rpc.request('GET_CHANNEL', { channel_id: channelId });
  if (currentChannel !== channelId) return; // on a déjà changé de salon entre-temps
  state.channel = { id: ch.id, name: ch.name, guildId: ch.guild_id };
  for (const vs of ch.voice_states || []) {
    const m = toMember(vs, ch.guild_id);
    state.members.set(m.id, m);
  }
  prunePeople(); // on garde les réglages de ceux qui t'ont suivi dans le nouveau salon
  for (const evt of CHANNEL_EVENTS) await rpc.request('SUBSCRIBE', { channel_id: channelId }, evt);
  console.log(`Vocal suivi : ${ch.name} (${state.members.size} personne(s))`);
  broadcast();
}

// Filet de sécurité : si Discord ne prévient pas qu'on a quitté le vocal, on revérifie.
function recheckSelectedChannel() {
  setTimeout(() => {
    if (!rpc) return;
    rpc.request('GET_SELECTED_VOICE_CHANNEL').then(sel => queueFollow(sel ? sel.id : null)).catch(() => {});
  }, 1200);
}

function onEvent(evt, data) {
  switch (evt) {
    case 'VOICE_CHANNEL_SELECT':
      queueFollow(data.channel_id || null);
      break;
    case 'VOICE_STATE_CREATE':
    case 'VOICE_STATE_UPDATE': {
      const m = toMember(data, state.channel && state.channel.guildId);
      state.members.set(m.id, m);
      broadcast();
      break;
    }
    case 'VOICE_STATE_DELETE':
      state.members.delete(data.user.id);
      state.speaking.delete(data.user.id);
      prunePeople();
      broadcast();
      if (data.user.id === state.selfId) recheckSelectedChannel();
      break;
    case 'SPEAKING_START':
      if (data.channel_id && data.channel_id !== currentChannel) return;
      state.speaking.add(data.user_id);
      broadcast();
      break;
    case 'SPEAKING_STOP':
      state.speaking.delete(data.user_id);
      broadcast();
      break;
  }
}

const sleep = ms => new Promise(r => setTimeout(r, ms));

// attend `ms`, mais repart tout de suite si les identifiants viennent d'être modifiés
async function pause(ms) {
  for (let t = 0; t < ms && !restartRequested; t += 250) await sleep(250);
}

async function run() {
  for (;;) {
    restartRequested = false;
    if (!hasConfig()) {
      setStatus('setup', 'Identifiants Discord manquants : ouvre la page de réglages pour les saisir.');
      await pause(3600000);
      continue;
    }
    try {
      rpc = new DiscordRPC(config.clientId);
      await rpc.connect();
      const auth = await authenticate(rpc);
      state.selfId = auth.user.id;
      state.user = auth.user.global_name || auth.user.username;
      setStatus('connected', `Connecté à Discord en tant que ${state.user}.`);
      rpc.onEvent = onEvent;
      await rpc.request('SUBSCRIBE', {}, 'VOICE_CHANNEL_SELECT');
      const selected = await rpc.request('GET_SELECTED_VOICE_CHANNEL');
      queueFollow(selected ? selected.id : null);
      await rpc.closed;
      if (!restartRequested) setStatus('no-discord', 'Connexion à Discord perdue, nouvelle tentative…');
    } catch (e) {
      if (rpc && rpc.sock) rpc.sock.destroy();
      if (!restartRequested) {
        if (e.code === 'NO_DISCORD') setStatus('no-discord', 'Discord introuvable : l\'appli Discord (desktop) est bien ouverte ?');
        else setStatus('error', e.message);
      }
    }
    currentChannel = null;
    resetState();
    prunePeople();
    broadcast();
    await pause(5000);
  }
}

// ---------- Mode démo ----------
function runDemo() {
  const names = ['CAILLOU', 'Zed main', 'Jungle Diff', 'mid or feed', 'Support Gap', 'ADC Enjoyer', 'Toplaner'];
  const add = (name, i) => {
    const id = String(100000000000000000n + BigInt(i) * 4194304n * 7n);
    state.members.set(id, { id, name, avatar: `https://cdn.discordapp.com/embed/avatars/${i % 6}.png`, mute: false, deaf: false });
    return id;
  };
  state.channel = { id: '1', name: 'Inhouse #12 — Équipe Bleue', guildId: '1' };
  const ids = names.slice(0, 5).map(add);
  state.selfId = ids[0];
  let next = 5;
  setInterval(() => {
    for (const id of state.members.keys()) {
      if (Math.random() < 0.25) state.speaking.has(id) ? state.speaking.delete(id) : state.speaking.add(id);
    }
    broadcast();
  }, 500);
  setInterval(() => {
    const keys = [...state.members.keys()];
    const r = Math.random();
    if (r < 0.35 && keys.length > 3) {
      const id = keys[1 + Math.floor(Math.random() * (keys.length - 1))];
      state.members.delete(id); state.speaking.delete(id);
      prunePeople();
    } else if (r < 0.7 && keys.length < 7) {
      add(names[next % names.length], next); next++;
    } else {
      const m = state.members.get(keys[Math.floor(Math.random() * keys.length)]);
      m.mute = !m.mute;
    }
    broadcast();
  }, 4000);
  console.log('Mode démo : faux membres actifs.');
}

DEMO ? runDemo() : run();
