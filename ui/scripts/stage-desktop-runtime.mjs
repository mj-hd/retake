import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

if (process.platform !== 'darwin') {
  throw new Error('Desktop runtime staging currently supports macOS only');
}

const uiDirectory = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const workerDirectory = path.resolve(uiDirectory, '../workers/web-capture');
const playwrightDirectory = path.join(workerDirectory, 'node_modules/playwright-core');
const browsersManifest = path.join(playwrightDirectory, 'browsers.json');

if (!fs.existsSync(browsersManifest)) {
  throw new Error('Install workers/web-capture dependencies before staging the desktop runtime');
}

const manifest = JSON.parse(fs.readFileSync(browsersManifest, 'utf8'));
const browser = manifest.browsers.find(({ name }) => name === 'chromium-headless-shell');
if (!browser) throw new Error('Playwright does not declare chromium-headless-shell');

const cacheRoot = process.env.PLAYWRIGHT_BROWSERS_PATH || path.join(os.homedir(), 'Library/Caches/ms-playwright');
const browserRoot = path.join(cacheRoot, `chromium_headless_shell-${browser.revision}`);
if (!fs.existsSync(browserRoot) || !fs.statSync(browserRoot).isDirectory()) {
  throw new Error(
    `Chromium headless shell ${browser.revision} is not installed. Run: cd workers/web-capture && npx playwright install chromium-headless-shell`,
  );
}

const sourceDirectory = fs
  .readdirSync(browserRoot, { withFileTypes: true })
  .find((entry) => entry.isDirectory() && entry.name.startsWith('chrome-headless-shell-'));
if (!sourceDirectory) throw new Error(`Could not find the Chromium runtime inside ${browserRoot}`);

const source = path.join(browserRoot, sourceDirectory.name);
const executable = path.join(source, 'chrome-headless-shell');
if (!fs.existsSync(executable)) throw new Error(`Chromium executable is missing at ${executable}`);

const resourcesDirectory = path.join(uiDirectory, 'src-tauri/resources');
const destinations = {
  chromium: path.join(resourcesDirectory, 'chromium'),
  runtime: path.join(resourcesDirectory, 'runtime'),
  worker: path.join(resourcesDirectory, 'workers/web-capture'),
  ui: path.join(resourcesDirectory, 'ui'),
  skill: path.join(resourcesDirectory, 'skills/retake'),
};
for (const destination of Object.values(destinations)) {
  fs.rmSync(destination, { recursive: true, force: true });
}

const uiBuild = path.join(uiDirectory, 'dist');
if (!fs.existsSync(path.join(uiBuild, 'index.html'))) {
  throw new Error('Build the UI first with `npm run build` before staging desktop assets');
}
fs.cpSync(uiBuild, path.join(destinations.ui, 'dist'), { recursive: true, dereference: true });

const skillSource = path.resolve(uiDirectory, '../.agents/skills/retake');
if (!fs.existsSync(path.join(skillSource, 'SKILL.md'))) {
  throw new Error(`Retake skill is missing at ${skillSource}`);
}
fs.cpSync(skillSource, destinations.skill, { recursive: true, dereference: true });

// Copy only the capture entry points and their installed production dependency
// tree; omit npm's command shims and any nested browser caches. Dereferencing
// package symlinks ensures every bundled resource stays inside the app bundle.
const workerFiles = ['index.js', 'review-window.js', 'pdf-map.js', 'code-map.js', 'package.json'];
for (const file of workerFiles) {
  const sourceFile = path.join(workerDirectory, file);
  if (!fs.existsSync(sourceFile)) throw new Error(`Required worker asset is missing: ${sourceFile}`);
  fs.mkdirSync(destinations.worker, { recursive: true });
  fs.copyFileSync(sourceFile, path.join(destinations.worker, file));
}
const sourceModules = path.join(workerDirectory, 'node_modules');
if (!fs.existsSync(sourceModules)) throw new Error(`Worker dependencies are missing: ${sourceModules}`);
const nativePrebuild = `darwin-${process.arch === 'arm64' ? 'arm64' : 'x64'}`;
fs.cpSync(sourceModules, path.join(destinations.worker, 'node_modules'), {
  recursive: true,
  dereference: true,
  filter: (entry) => {
    const relative = path.relative(sourceModules, entry);
    const segments = relative.split(path.sep);
    if (segments.includes('.bin') || segments.includes('.local-browsers')) return false;
    const prebuilds = segments.indexOf('prebuilds');
    return prebuilds < 0 || segments.length === prebuilds + 1 || segments[prebuilds + 1] === nativePrebuild;
  },
});

// Ship the Node executable used for all worker processes. It is copied from
// the Node running this staging script, so the finished app has no npm/Node
// PATH dependency.
fs.mkdirSync(destinations.runtime, { recursive: true });
const nodeSource = fs.realpathSync(process.execPath);
const nodeDestination = path.join(destinations.runtime, 'node');
fs.copyFileSync(nodeSource, nodeDestination);
fs.chmodSync(nodeDestination, 0o755);

fs.mkdirSync(destinations.chromium, { recursive: true });
fs.cpSync(source, destinations.chromium, { recursive: true, dereference: true });
fs.chmodSync(path.join(destinations.chromium, 'chrome-headless-shell'), 0o755);

const size = fs.statSync(executable).size / 1024 / 1024;
const nodeSize = fs.statSync(nodeDestination).size / 1024 / 1024;
console.log(
  `Staged Node (${nodeSize.toFixed(1)} MiB), workers, and Chromium ${browser.browserVersion} (${size.toFixed(1)} MiB)`,
);
