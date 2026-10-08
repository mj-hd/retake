import { invoke } from '@tauri-apps/api/core';

export type ClientId = 'claude_code' | 'claude_desktop' | 'codex' | 'opencode' | 'gemini';

/**
 * Rust serializes `ClientId::OpenCode` as `open_code`, while the installer
 * commands accept the shorter `opencode` alias. Accept both spellings so the
 * status list never contains an id the UI cannot label.
 */
export function normalizeClientId(value: string): ClientId {
  return (value === 'open_code' ? 'opencode' : value) as ClientId;
}
export type InstallState = 'unavailable' | 'missing' | 'installed' | 'outdated' | 'invalid';

export interface ClientStatus {
  id: ClientId;
  state: InstallState;
  config_path: string;
  version: string | null;
  message: string | null;
}

export function desktopAvailable(): boolean {
  return '__TAURI_INTERNALS__' in window;
}

function normalize(clients: ClientStatus[]): ClientStatus[] {
  return clients.map((client) => ({ ...client, id: normalizeClientId(client.id) }));
}

export function installerStatus(): Promise<ClientStatus[]> {
  return invoke<ClientStatus[]>('installer_status').then(normalize);
}

export function installClients(ids: ClientId[]): Promise<ClientStatus[]> {
  return invoke<ClientStatus[]>('install_clients', { ids }).then(normalize);
}

export function removeClients(ids: ClientId[]): Promise<ClientStatus[]> {
  return invoke<ClientStatus[]>('remove_clients', { ids }).then(normalize);
}

export function quitDesktopApp(): Promise<void> {
  return invoke('quit_app');
}

export function actionableClients(clients: ClientStatus[]): ClientId[] {
  return clients
    .filter((client) => client.state === 'missing' || client.state === 'outdated')
    .map((client) => client.id);
}
