import { inspectLiveRepairReadiness, inspectProviderReadiness, summarizeProviderReadiness } from './provider-readiness';

const assert = {
  equal(actual: unknown, expected: unknown): void {
    if (actual !== expected) {
      throw new Error(`Expected ${String(expected)}, got ${String(actual)}`);
    }
  },
};

function test(name: string, fn: () => Promise<void> | void): void {
  Promise.resolve()
    .then(fn)
    .then(() => {
      console.log(`ok - ${name}`);
    })
    .catch((error) => {
      console.error(`not ok - ${name}`);
      throw error;
    });
}

test('summarizeProviderReadiness prefers inline keys when available', () => {
  const readiness = summarizeProviderReadiness({
    provider: {
      kind: 'anthropic',
      name: 'anthropic',
      apiKey: 'sk-inline',
      baseUrl: 'https://api.anthropic.com',
      model: 'claude-sonnet-4-6',
      thinking: '',
    },
    hasSecureStoreKey: false,
  });

  assert.equal(readiness.ready, true);
  assert.equal(readiness.source, 'inline');
});

test('summarizeProviderReadiness reports secure store readiness when settings key is empty', () => {
  const readiness = summarizeProviderReadiness({
    provider: {
      kind: 'openai',
      name: 'deepseek',
      apiKey: '',
      baseUrl: 'https://api.deepseek.com/v1',
      model: 'deepseek-v4-pro',
    },
    hasSecureStoreKey: true,
  });

  assert.equal(readiness.ready, true);
  assert.equal(readiness.source, 'secure_store');
});

test('summarizeProviderReadiness reports missing credentials when neither source is available', () => {
  const readiness = summarizeProviderReadiness({
    provider: {
      kind: 'openai',
      name: 'deepseek',
      apiKey: '',
      baseUrl: 'https://api.deepseek.com/v1',
      model: 'deepseek-v4-pro',
    },
    hasSecureStoreKey: false,
  });

  assert.equal(readiness.ready, false);
  assert.equal(readiness.source, 'missing');
  assert.equal(readiness.reason, 'No provider API key available in settings or secure storage.');
});

test('inspectProviderReadiness short-circuits to inline readiness when settings key exists', async () => {
  let lookedUpSecureStore = false;

  const readiness = await inspectProviderReadiness({
    provider: {
      kind: 'anthropic',
      name: 'anthropic',
      apiKey: 'sk-inline',
      baseUrl: 'https://api.anthropic.com',
      model: 'claude-sonnet-4-6',
      thinking: '',
    },
    mockMode: false,
    hasSecureStoreKey: async () => {
      lookedUpSecureStore = true;
      return true;
    },
  });

  assert.equal(readiness.source, 'inline');
  assert.equal(lookedUpSecureStore, false);
});

test('inspectProviderReadiness reports missing in mock mode without secure store lookup', async () => {
  let lookedUpSecureStore = false;

  const readiness = await inspectProviderReadiness({
    provider: {
      kind: 'openai',
      name: 'deepseek',
      apiKey: '',
      baseUrl: 'https://api.deepseek.com/v1',
      model: 'deepseek-v4-pro',
    },
    mockMode: true,
    hasSecureStoreKey: async () => {
      lookedUpSecureStore = true;
      return true;
    },
  });

  assert.equal(readiness.source, 'missing');
  assert.equal(lookedUpSecureStore, false);
});

test('inspectProviderReadiness uses secure store result when settings key is empty', async () => {
  const readiness = await inspectProviderReadiness({
    provider: {
      kind: 'openai',
      name: 'deepseek',
      apiKey: '',
      baseUrl: 'https://api.deepseek.com/v1',
      model: 'deepseek-v4-pro',
    },
    mockMode: false,
    hasSecureStoreKey: async () => true,
  });

  assert.equal(readiness.source, 'secure_store');
  assert.equal(readiness.ready, true);
});

test('inspectProviderReadiness degrades to missing when secure store lookup fails', async () => {
  const readiness = await inspectProviderReadiness({
    provider: {
      kind: 'openai',
      name: 'deepseek',
      apiKey: '',
      baseUrl: 'https://api.deepseek.com/v1',
      model: 'deepseek-v4-pro',
    },
    mockMode: false,
    hasSecureStoreKey: async () => {
      throw new Error('credential_has failed');
    },
  });

  assert.equal(readiness.source, 'missing');
  assert.equal(readiness.ready, false);
});

test('inspectLiveRepairReadiness blocks mock browser mode even when an inline key exists', async () => {
  const readiness = await inspectLiveRepairReadiness({
    settings: {
      activeProvider: 'deepseek',
      providers: [{
        kind: 'openai',
        name: 'deepseek',
        apiKey: 'sk-inline',
        baseUrl: 'https://api.deepseek.com/v1',
        model: 'deepseek-v4-pro',
      }],
      projectPath: '/mock/nebula-project',
      agent: { temperature: 0.7, maxSteps: 50, contextWindow: 0, chatMode: 'general' },
      display: { defaultViewMode: 'standard', language: 'zh' },
    },
    workspacePath: '/mock/nebula-project',
    mockMode: true,
  });

  assert.equal(readiness.mode, 'mock_browser');
  assert.equal(readiness.eligible, false);
});

test('inspectLiveRepairReadiness allows tauri workspace mode when provider is ready', async () => {
  const readiness = await inspectLiveRepairReadiness({
    settings: {
      activeProvider: 'anthropic',
      providers: [{
        kind: 'anthropic',
        name: 'anthropic',
        apiKey: 'sk-inline',
        baseUrl: 'https://api.anthropic.com',
        model: 'claude-sonnet-4-6',
        thinking: '',
      }],
      projectPath: '/Users/liupeicheng/Documents/New project 13/repo',
      agent: { temperature: 0.7, maxSteps: 50, contextWindow: 0, chatMode: 'general' },
      display: { defaultViewMode: 'standard', language: 'zh' },
    },
    workspacePath: '/Users/liupeicheng/Documents/New project 13/repo',
    mockMode: false,
  });

  assert.equal(readiness.mode, 'tauri_workspace');
  assert.equal(readiness.eligible, true);
});
