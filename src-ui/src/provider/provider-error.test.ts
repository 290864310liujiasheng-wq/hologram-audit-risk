import {
  ProviderRequestError,
  classifyProviderFailure,
} from './types';

const assert = {
  equal(actual: unknown, expected: unknown): void {
    if (actual !== expected) {
      throw new Error(`Expected ${String(expected)}, got ${String(actual)}`);
    }
  },
};

function test(name: string, fn: () => void): void {
  try {
    fn();
    console.log(`ok - ${name}`);
  } catch (error) {
    console.error(`not ok - ${name}`);
    throw error;
  }
}

test('classifyProviderFailure maps 401 to provider_auth_invalid', () => {
  const error = classifyProviderFailure({
    provider_name: 'anthropic',
    message: 'authentication failed',
    status: 401,
  });

  assert.equal(error.code, 'provider_auth_invalid');
  assert.equal(error.retryable, false);
});

test('classifyProviderFailure maps 429 to rate_limited', () => {
  const error = classifyProviderFailure({
    provider_name: 'openai-compatible',
    message: 'rate limit exceeded',
    status: 429,
  });

  assert.equal(error.code, 'rate_limited');
  assert.equal(error.retryable, true);
});

test('classifyProviderFailure maps 503 to provider_upstream_failed', () => {
  const error = classifyProviderFailure({
    provider_name: 'anthropic',
    message: 'service unavailable',
    status: 503,
  });

  assert.equal(error.code, 'provider_upstream_failed');
  assert.equal(error.retryable, true);
});

test('classifyProviderFailure maps ENOTFOUND to network_unreachable', () => {
  const error = classifyProviderFailure({
    provider_name: 'openai-compatible',
    message: 'request failed: getaddrinfo ENOTFOUND api.example.com',
  });

  assert.equal(error.code, 'network_unreachable');
  assert.equal(error.retryable, true);
});

test('classifyProviderFailure maps certificate revoked to tls_cert_revoked', () => {
  const error = classifyProviderFailure({
    provider_name: 'anthropic',
    message: 'x509: certificate has been revoked',
  });

  assert.equal(error.code, 'tls_cert_revoked');
  assert.equal(error.retryable, false);
});

test('classifyProviderFailure maps proxy refusal to proxy_rejected', () => {
  const error = classifyProviderFailure({
    provider_name: 'openai-compatible',
    message: 'proxy connect ECONNREFUSED 127.0.0.1:7890',
  });

  assert.equal(error.code, 'proxy_rejected');
  assert.equal(error.retryable, true);
});

test('classifyProviderFailure maps HTTP 407 to proxy_rejected', () => {
  const error = classifyProviderFailure({
    provider_name: 'openai-compatible',
    message: 'proxy authentication required',
    status: 407,
  });

  assert.equal(error.code, 'proxy_rejected');
  assert.equal(error.retryable, true);
});

test('classifyProviderFailure maps ECONNRESET to connection_interrupted', () => {
  const error = classifyProviderFailure({
    provider_name: 'anthropic',
    message: 'request failed: read ECONNRESET',
  });

  assert.equal(error.code, 'connection_interrupted');
  assert.equal(error.retryable, true);
});

test('classifyProviderFailure maps unexpected eof to connection_interrupted', () => {
  const error = classifyProviderFailure({
    provider_name: 'anthropic',
    message: 'upstream stream terminated unexpectedly: unexpected EOF while reading response body',
  });

  assert.equal(error.code, 'connection_interrupted');
  assert.equal(error.retryable, true);
});

test('classifyProviderFailure maps issuer certificate chain failures to tls_handshake_failed', () => {
  const error = classifyProviderFailure({
    provider_name: 'openai-compatible',
    message: 'request failed: unable to get local issuer certificate',
  });

  assert.equal(error.code, 'tls_handshake_failed');
  assert.equal(error.retryable, false);
});

test('ProviderRequestError preserves provider metadata', () => {
  const error = new ProviderRequestError({
    provider_name: 'anthropic',
    code: 'timeout',
    message: 'timeout after 30000ms',
    retryable: true,
    status: 408,
  });

  assert.equal(error.provider_name, 'anthropic');
  assert.equal(error.status, 408);
});
