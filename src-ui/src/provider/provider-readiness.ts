import { invoke, isMockMode } from '../bridge';
import type { AppSettings, ProviderSettings } from '../settings';
import { getActiveProvider } from '../settings';

export interface ProviderReadiness {
  provider_name: string;
  model: string;
  source: 'inline' | 'secure_store' | 'missing';
  ready: boolean;
  reason: string;
  has_inline_key: boolean;
  has_secure_store_key: boolean;
}

export interface LiveRepairReadiness {
  mode: 'mock_browser' | 'tauri_workspace';
  eligible: boolean;
  reason: string;
  workspace_path: string;
  provider: ProviderReadiness;
}

export function summarizeProviderReadiness(input: {
  provider: ProviderSettings;
  hasSecureStoreKey: boolean;
}): ProviderReadiness {
  const hasInlineKey = Boolean(input.provider.apiKey.trim());

  if (hasInlineKey) {
    return {
      provider_name: input.provider.name,
      model: input.provider.model,
      source: 'inline',
      ready: true,
      reason: 'Provider API key is available in settings.',
      has_inline_key: true,
      has_secure_store_key: input.hasSecureStoreKey,
    };
  }

  if (input.hasSecureStoreKey) {
    return {
      provider_name: input.provider.name,
      model: input.provider.model,
      source: 'secure_store',
      ready: true,
      reason: 'Provider API key can be restored from secure storage.',
      has_inline_key: false,
      has_secure_store_key: true,
    };
  }

  return {
    provider_name: input.provider.name,
    model: input.provider.model,
    source: 'missing',
    ready: false,
    reason: 'No provider API key available in settings or secure storage.',
    has_inline_key: false,
    has_secure_store_key: false,
  };
}

export async function inspectActiveProviderReadiness(settings: AppSettings): Promise<ProviderReadiness> {
  const provider = getActiveProvider(settings);
  return inspectProviderReadiness({
    provider,
    mockMode: isMockMode(),
    hasSecureStoreKey: async () => invoke<boolean>('credential_has', { provider: provider.name }),
  });
}

export async function inspectLiveRepairReadiness(input: {
  settings: AppSettings;
  workspacePath: string;
  mockMode?: boolean;
}): Promise<LiveRepairReadiness> {
  const providerConfig = getActiveProvider(input.settings);
  const mockMode = input.mockMode ?? isMockMode();
  const provider = await inspectProviderReadiness({
    provider: providerConfig,
    mockMode,
    hasSecureStoreKey: async () => invoke<boolean>('credential_has', { provider: providerConfig.name }),
  });
  const runtimeMockMode = mockMode || input.workspacePath.startsWith('/mock/');

  if (runtimeMockMode) {
    return {
      mode: 'mock_browser',
      eligible: false,
      reason: 'Current session is using mock browser data, so live repair evidence cannot be produced here.',
      workspace_path: input.workspacePath,
      provider,
    };
  }

  if (!provider.ready) {
    return {
      mode: 'tauri_workspace',
      eligible: false,
      reason: provider.reason,
      workspace_path: input.workspacePath,
      provider,
    };
  }

  return {
    mode: 'tauri_workspace',
    eligible: true,
    reason: 'Live repair proposal generation can run against the current workspace.',
    workspace_path: input.workspacePath,
    provider,
  };
}

export async function inspectProviderReadiness(input: {
  provider: ProviderSettings;
  mockMode: boolean;
  hasSecureStoreKey: () => Promise<boolean>;
}): Promise<ProviderReadiness> {
  const provider = input.provider;
  if (provider.apiKey.trim()) {
    return summarizeProviderReadiness({
      provider,
      hasSecureStoreKey: false,
    });
  }

  if (input.mockMode) {
    return summarizeProviderReadiness({
      provider,
      hasSecureStoreKey: false,
    });
  }

  const hasSecureStoreKey = await input.hasSecureStoreKey()
    .catch(() => false);

  return summarizeProviderReadiness({
    provider,
    hasSecureStoreKey,
  });
}
