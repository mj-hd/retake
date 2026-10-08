import { useCallback, useEffect, useState } from 'react';
import BrandMark from './BrandMark.tsx';
import {
  actionableClients,
  desktopAvailable,
  installClients,
  installerStatus,
  openClaudeSkills,
  quitDesktopApp,
  removeClients,
  restartClient,
} from './desktopInstaller.ts';
import type { ClientId, ClientStatus } from './desktopInstaller.ts';
import { initialLocale } from './i18n.ts';

type Text = {
  title: string;
  lead: string;
  installAll: string;
  quit: string;
  install: string;
  update: string;
  installed: string;
  uninstall: string;
  uninstalled: string;
  restartRequired: string;
  restartNow: string;
  restarted: string;
  unavailableApp: string;
  failed: string;
  removeFailed: string;
  restartFailed: string;
  unreadable: string;
  skillReady: string;
  skillFailed: string;
  skillManualRemoval: string;
  skillSettings: string;
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
    uninstall: 'アンインストール',
    uninstalled: 'アンインストールしました。エージェントを再起動してください',
    restartRequired: '再起動が必要です',
    restartNow: '再起動する',
    restarted: '再起動しました',
    unavailableApp: 'デスクトップアプリとして起動してください',
    failed: '登録できませんでした',
    removeFailed: 'アンインストールできませんでした',
    restartFailed: '再起動できませんでした',
    unreadable: '設定を読めません',
    skillReady: 'retake.skillをClaudeで開きました。表示された画面でインストールを確認してください',
    skillFailed: 'Skillの準備ができませんでした',
    skillManualRemoval: 'Skillは手動で削除してください。',
    skillSettings: '設定はこちら',
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
    uninstall: 'Uninstall',
    uninstalled: 'Uninstalled. Restart the agent to finish',
    restartRequired: 'Restart required',
    restartNow: 'Restart now',
    restarted: 'Restarted',
    unavailableApp: 'Open this screen from the Retake app',
    failed: 'Could not register',
    removeFailed: 'Could not uninstall',
    restartFailed: 'Could not restart',
    unreadable: 'Config unreadable',
    skillReady: 'Opened retake.skill in Claude. Confirm the installation in the window that appeared',
    skillFailed: 'Could not prepare the skill',
    skillManualRemoval: 'Remove the Skill manually.',
    skillSettings: 'Open settings',
    clients: {
      claude_code: ['Claude Code', 'CLI'],
      claude_desktop: ['Claude Desktop', 'Desktop'],
      codex: ['Codex', 'CLI / Desktop'],
      opencode: ['OpenCode', 'CLI / Desktop'],
      gemini: ['Gemini', 'CLI'],
    },
  },
};

function actionLabel(client: ClientStatus, text: Text): string | null {
  if (client.state === 'missing') return text.install;
  if (client.state === 'outdated') return text.update;
  return null;
}

function statusLabel(client: ClientStatus, needsRestart: boolean, text: Text): string | null {
  if (client.state === 'invalid') return text.unreadable;
  if (needsRestart) return text.restartRequired;
  return versionLabel(client.version) ?? (client.state === 'installed' ? text.installed : null);
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
  const [notice, setNotice] = useState<string | null>(null);

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

  useEffect(() => {
    const refreshOnFocus = () => refresh().catch(() => undefined);
    window.addEventListener('focus', refreshOnFocus);
    return () => window.removeEventListener('focus', refreshOnFocus);
  }, [refresh]);

  const run = useCallback(
    async (action: (ids: ClientId[]) => Promise<ClientStatus[]>, ids: ClientId[], operation: 'install' | 'remove') => {
      setBusy(true);
      setError(null);
      setNotice(null);
      try {
        const next = await action(ids);
        setClients((current) => current.map((client) => next.find((updated) => updated.id === client.id) ?? client));
        setRestartRequired((current) => {
          const changed = next
            .filter((client) => client.state !== 'invalid' && client.state !== 'unavailable')
            .map((client) => client.id);
          if (changed.length === 0) return current;
          return new Set([...current, ...changed]);
        });
        if (next.some((client) => client.message)) {
          setError(
            operation === 'remove'
              ? text.removeFailed
              : next.some((client) => client.id === 'claude_desktop' && client.message)
                ? text.skillFailed
                : text.failed,
          );
        } else if (operation === 'remove') {
          setNotice(text.uninstalled);
        } else if (next.some((client) => client.id === 'claude_desktop' && client.skill_state === 'pending')) {
          setNotice(text.skillReady);
        }
      } catch {
        setError(operation === 'remove' ? text.removeFailed : text.failed);
      } finally {
        setBusy(false);
      }
    },
    [text.failed, text.removeFailed, text.skillFailed, text.skillReady, text.uninstalled],
  );

  const pending = actionableClients(clients);
  const claudeSkillStateMayChange = clients.some(
    (client) =>
      client.id === 'claude_desktop' &&
      (client.skill_state === 'pending' ||
        (client.state === 'missing' && (client.skill_state === 'installed' || client.skill_state === 'outdated'))),
  );

  useEffect(() => {
    if (!claudeSkillStateMayChange) return;
    const timer = window.setInterval(() => {
      installerStatus()
        .then(setClients)
        .catch(() => undefined);
    }, 1500);
    return () => window.clearInterval(timer);
  }, [claudeSkillStateMayChange]);

  const restart = useCallback(
    async (id: ClientId) => {
      setBusy(true);
      setError(null);
      setNotice(null);
      try {
        await restartClient(id);
        setRestartRequired((current) => {
          const next = new Set(current);
          next.delete(id);
          return next;
        });
        await refresh();
        setNotice(text.restarted);
      } catch {
        setError(text.restartFailed);
      } finally {
        setBusy(false);
      }
    },
    [refresh, text.restartFailed, text.restarted],
  );

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
            const action = actionLabel(client, text);
            const needsRestart = restartRequired.has(client.id);
            const canAct =
              action !== null &&
              !(client.state === 'installed' && client.skill_state === 'installed') &&
              client.state !== 'unavailable';
            const updateAvailable = client.state === 'outdated' || client.skill_state === 'outdated';
            const canRemove = client.state === 'installed' && !updateAvailable;
            const needsManualSkillRemoval =
              client.id === 'claude_desktop' &&
              client.state === 'missing' &&
              (client.skill_state === 'installed' || client.skill_state === 'outdated');
            return (
              <li className="desktop-client" key={client.id} data-state={client.state}>
                <span className="desktop-client-label">
                  <strong>{name}</strong>
                  <small>{detail}</small>
                </span>
                <em className={needsRestart || needsManualSkillRemoval ? 'desktop-restart-note' : undefined}>
                  {needsManualSkillRemoval ? (
                    <>
                      {text.skillManualRemoval}{' '}
                      <button
                        className="desktop-text-link"
                        type="button"
                        onClick={() => openClaudeSkills().catch(() => setError(text.failed))}
                      >
                        {text.skillSettings}
                      </button>
                    </>
                  ) : (
                    <>
                      {statusLabel(client, needsRestart, text)}
                      {needsRestart && client.restartable ? (
                        <>
                          {' '}
                          <button className="desktop-text-link" type="button" onClick={() => restart(client.id)}>
                            {text.restartNow}
                          </button>
                        </>
                      ) : null}
                    </>
                  )}
                </em>
                {action || canRemove ? (
                  <span className="desktop-client-actions">
                    {canRemove ? (
                      <button
                        className="button button-quiet"
                        disabled={busy}
                        onClick={() => run(removeClients, [client.id], 'remove')}
                      >
                        {text.uninstall}
                      </button>
                    ) : null}
                    {action ? (
                      <button
                        className={`button ${canAct ? 'button-primary' : 'button-quiet'}`}
                        disabled={busy || !canAct}
                        onClick={() => run(installClients, [client.id], 'install')}
                      >
                        <span className="desktop-action-label" key={action}>
                          {action}
                        </span>
                      </button>
                    ) : null}
                  </span>
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
            onClick={() => run(installClients, pending, 'install')}
          >
            {text.installAll}
          </button>
        </div>
        {/* Reserved so an appearing error never reflows the list above. */}
        <div className="desktop-setup-status" role="status">
          {error ?? notice}
        </div>
      </section>
    </main>
  );
}
