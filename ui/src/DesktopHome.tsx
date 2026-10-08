import { useCallback, useEffect, useState } from 'react';
import BrandMark from './BrandMark.tsx';
import {
  actionableClients,
  desktopAvailable,
  installClients,
  installerStatus,
  quitDesktopApp,
} from './desktopInstaller.ts';
import type { ClientId, ClientStatus, InstallState } from './desktopInstaller.ts';
import { initialLocale } from './i18n.ts';

type Text = {
  title: string;
  lead: string;
  installAll: string;
  quit: string;
  install: string;
  update: string;
  installed: string;
  restartRequired: string;
  unavailableApp: string;
  failed: string;
  unreadable: string;
  clients: Record<ClientId, readonly [name: string, detail: string]>;
};

const copy: Record<'ja' | 'en', Text> = {
  ja: {
    title: 'RetakeをMCPとして登録する',
    lead: 'インストール後、エージェントに「retakeして」と伝えるだけで、画面やファイルをレビューできます',
    installAll: 'すべてインストール',
    quit: 'Retakeを終了',
    install: 'インストール',
    update: 'アップデート',
    installed: 'インストール済み',
    restartRequired: '再起動が必要です',
    unavailableApp: 'デスクトップアプリとして起動してください',
    failed: '登録できませんでした',
    unreadable: '設定を読めません',
    clients: {
      claude_code: ['Claude Code', 'CLI'],
      claude_desktop: ['Claude Desktop', 'Desktop'],
      codex: ['Codex', 'CLI / Desktop'],
      opencode: ['OpenCode', 'CLI / Desktop'],
      gemini: ['Gemini', 'CLI'],
    },
  },
  en: {
    title: 'Register Retake as an MCP server',
    lead: 'After installation, ask your agent to retake something and review screens or files right away',
    installAll: 'Install all',
    quit: 'Quit Retake',
    install: 'Install',
    update: 'Update',
    installed: 'Installed',
    restartRequired: 'Restart required',
    unavailableApp: 'Open this screen from the Retake app',
    failed: 'Could not register',
    unreadable: 'Config unreadable',
    clients: {
      claude_code: ['Claude Code', 'CLI'],
      claude_desktop: ['Claude Desktop', 'Desktop'],
      codex: ['Codex', 'CLI / Desktop'],
      opencode: ['OpenCode', 'CLI / Desktop'],
      gemini: ['Gemini', 'CLI'],
    },
  },
};

function actionLabel(state: InstallState, text: Text): string | null {
  if (state === 'missing') return text.install;
  if (state === 'outdated') return text.update;
  if (state === 'installed') return text.installed;
  return null;
}

function versionLabel(version: string | null): string | null {
  if (!version) return null;
  return version.startsWith('v') ? version : `v${version}`;
}

export default function DesktopHome() {
  const text = copy[initialLocale()];
  const [clients, setClients] = useState<ClientStatus[]>([]);
  const [restartRequired, setRestartRequired] = useState<Set<ClientId>>(() => new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const syncTheme = () => document.documentElement.setAttribute('data-theme', media.matches ? 'dark' : 'light');
    syncTheme();
    media.addEventListener('change', syncTheme);
    return () => media.removeEventListener('change', syncTheme);
  }, []);

  const refresh = useCallback(async () => {
    if (!desktopAvailable()) {
      setError(text.unavailableApp);
      return;
    }
    setClients(await installerStatus());
  }, [text.unavailableApp]);

  useEffect(() => {
    refresh().catch(() => setError(text.failed));
  }, [refresh, text.failed]);

  const run = useCallback(
    async (action: (ids: ClientId[]) => Promise<ClientStatus[]>, ids: ClientId[]) => {
      setBusy(true);
      setError(null);
      try {
        const next = await action(ids);
        setClients((current) => current.map((client) => next.find((updated) => updated.id === client.id) ?? client));
        setRestartRequired((current) => {
          const installed = next.filter((client) => client.state === 'installed').map((client) => client.id);
          if (installed.length === 0) return current;
          return new Set([...current, ...installed]);
        });
        if (next.some((client) => client.state === 'invalid')) setError(text.failed);
      } catch {
        setError(text.failed);
      } finally {
        setBusy(false);
      }
    },
    [text.failed],
  );

  const pending = actionableClients(clients);

  return (
    <main className="desktop-home">
      <section className="desktop-setup">
        <header className="desktop-home-brand" aria-label="Retake">
          <BrandMark />
        </header>
        <div className="desktop-intro">
          <h1>{text.title}</h1>
          <p>{text.lead}</p>
        </div>
        <ul className="desktop-clients">
          {clients.map((client) => {
            const [name, detail] = text.clients[client.id];
            const action = actionLabel(client.state, text);
            const needsRestart = restartRequired.has(client.id);
            return (
              <li className="desktop-client" key={client.id} data-state={client.state}>
                <span className="desktop-client-label">
                  <strong>{name}</strong>
                  <small>{detail}</small>
                </span>
                <em className={needsRestart ? 'desktop-restart-note' : undefined}>
                  {client.state === 'invalid'
                    ? text.unreadable
                    : needsRestart
                      ? text.restartRequired
                      : versionLabel(client.version)}
                </em>
                {action ? (
                  <button
                    className={`button ${client.state === 'installed' ? 'button-quiet' : 'button-primary'}`}
                    disabled={busy || client.state === 'installed'}
                    onClick={() => run(installClients, [client.id])}
                  >
                    <span className="desktop-action-label" key={action}>
                      {action}
                    </span>
                  </button>
                ) : (
                  <span />
                )}
              </li>
            );
          })}
        </ul>
        <div className="desktop-setup-actions">
          <button className="button button-quiet" disabled={busy} onClick={() => void quitDesktopApp()}>
            {text.quit}
          </button>
          <button
            className="button button-primary"
            disabled={busy || pending.length === 0}
            onClick={() => run(installClients, pending)}
          >
            {text.installAll}
          </button>
        </div>
        {/* Reserved so an appearing error never reflows the list above. */}
        <div className="desktop-setup-status" role="status">
          {error}
        </div>
      </section>
    </main>
  );
}
