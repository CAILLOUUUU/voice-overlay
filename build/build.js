// Fabrique "Voice-Overlay-CAILLOU-FEAT-CLAUDE.exe" (Windows x64) à partir des sources.
//   cd build && npm install && node build.js
// Marche depuis Windows, macOS ou Linux. Il faut la même version de Node que NODE_VERSION.
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');
const ResEdit = require('resedit');

const NODE_VERSION = process.versions.node;
const ROOT = path.join(__dirname, '..');
const CACHE = path.join(__dirname, '.cache');
const OUT = path.join(ROOT, 'dist', 'Voice-Overlay-CAILLOU-FEAT-CLAUDE.exe');
const FUSE = 'NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2';
fs.mkdirSync(CACHE, { recursive: true });
fs.mkdirSync(path.dirname(OUT), { recursive: true });

// 1. node.exe pour Windows (récupéré depuis npm)
const nodeExe = path.join(CACHE, `node-${NODE_VERSION}.exe`);
if (!fs.existsSync(nodeExe)) {
  execSync(`npm pack node-win-x64@${NODE_VERSION}`, { cwd: CACHE, stdio: 'ignore' });
  execSync(`tar xzf node-win-x64-${NODE_VERSION}.tgz`, { cwd: CACHE });
  fs.copyFileSync(path.join(CACHE, 'package', 'bin', 'node.exe'), nodeExe);
}

// 2. Le programme + ses fichiers, regroupés en un seul bloc
const assets = { 'LICENCES-TIERS.txt': 'LICENCES-TIERS.txt', 'overlay.html': 'overlay.html', 'settings.html': 'settings.html', 'icon.png': 'icon.png', 'icon.ico': 'icon.ico' };
for (const f of fs.readdirSync(path.join(ROOT, 'webfonts'))) assets['webfonts/' + f] = 'webfonts/' + f;
for (const k of Object.keys(assets)) assets[k] = path.join(ROOT, assets[k]);
const blob = path.join(CACHE, 'sea.blob');
fs.writeFileSync(path.join(CACHE, 'sea-config.json'), JSON.stringify({
  main: path.join(ROOT, 'server.js'), output: blob, disableExperimentalSEAWarning: true, assets,
}));
execSync(`"${process.execPath}" --experimental-sea-config sea-config.json`, { cwd: CACHE, stdio: 'ignore' });

// 3. Icône, nom du programme, et pas de fenêtre console
const exe = ResEdit.NtExecutable.from(fs.readFileSync(nodeExe), { ignoreCert: true });
const res = ResEdit.NtExecutableResource.from(exe);
const group = ResEdit.Resource.IconGroupEntry.fromEntries(res.entries)[0];
const ico = ResEdit.Data.IconFile.from(fs.readFileSync(path.join(ROOT, 'icon.ico')));
ResEdit.Resource.IconGroupEntry.replaceIconsForResource(res.entries, group.id, group.lang, ico.icons.map(i => i.data));
const vi = ResEdit.Resource.VersionInfo.fromEntries(res.entries)[0];
const lang = vi.getAllLanguagesForStringValues()[0];
vi.setStringValues(lang, {
  ProductName: 'Voice Overlay - Caillou feat Claude', FileDescription: 'Voice Overlay - Caillou feat Claude',
  CompanyName: 'Caillou', OriginalFilename: 'Voice-Overlay-CAILLOU-FEAT-CLAUDE.exe', InternalName: 'VoiceOverlay',
  LegalCopyright: 'Caillou feat Claude (moteur : Node.js)',
});
vi.outputToResourceEntries(res.entries);
const [maj, min, pat] = require('fs').readFileSync(path.join(ROOT, 'server.js'), 'utf8').match(/const VERSION = '(\d+)\.(\d+)\.(\d+)'/).slice(1).map(Number);
vi.setFileVersion(maj, min, pat, 0, lang.lang);
vi.setProductVersion(maj, min, pat, 0, lang.lang);
vi.outputToResourceEntries(res.entries);
res.outputResource(exe);
exe.newHeader.optionalHeader.subsystem = 2;
fs.writeFileSync(OUT, Buffer.from(exe.generate()));

// 4. On glisse le bloc dans l'exe
execSync(`npx postject "${OUT}" NODE_SEA_BLOB "${blob}" --sentinel-fuse ${FUSE}`, { cwd: __dirname, stdio: 'ignore' });
console.log('OK ->', OUT, `(${(fs.statSync(OUT).size / 1e6).toFixed(0)} Mo)`);
