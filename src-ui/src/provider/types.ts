// Provider 抽象层 — 统一 Message / Chunk / ToolCall，抹平 Anthropic 和 OpenAI 的 API 差异

export type Role = 'system' | 'user' | 'assistant' | 'tool';

export interface Message {
  role: Role;
  content: string;
  /** thinking-mode chain-of-thought, round-tripped on multi-turn */
  reasoning_content?: string;
  /** opaque provider-issued proof for reasoning (Anthropic thinking signature) */
  reasoning_signature?: string;
  /** set by assistant */
  tool_calls?: ToolCall[];
  /** links a tool result to its call */
  tool_call_id?: string;
  /** tool message: tool name */
  name?: string;
}

export interface ToolCall {
  id: string;
  name: string;
  arguments: string; // raw JSON
}

export interface ToolSchema {
  name: string;
  description: string;
  parameters: Record<string, unknown>; // JSON Schema
}

export interface Request {
  messages: Message[];
  tools: ToolSchema[];
  temperature: number;
  max_tokens: number;
}

export enum ChunkType {
  Text = 0,
  Reasoning = 1,
  ToolCallStart = 2,
  ToolCall = 3,
  Usage = 4,
  Done = 5,
  Error = 6,
}

export interface Usage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  cache_hit_tokens: number;
  cache_miss_tokens: number;
  reasoning_tokens: number;
  finish_reason: string; // "stop", "tool_calls", "length", "content_filter"
}

export interface Chunk {
  type: ChunkType;
  text?: string;
  signature?: string; // ChunkReasoning: Anthropic thinking signature
  tool_call?: ToolCall; // ChunkToolCallStart (id+name only) or ChunkToolCall (complete)
  usage?: Usage;
  err?: Error;
}

export type ProviderFailureCode =
  | 'provider_auth_invalid'
  | 'rate_limited'
  | 'timeout'
  | 'provider_upstream_failed'
  | 'network_unreachable'
  | 'tls_handshake_failed'
  | 'tls_cert_revoked'
  | 'proxy_rejected'
  | 'connection_interrupted'
  | 'provider_unavailable';

export class ProviderRequestError extends Error {
  readonly provider_name: string;
  readonly code: ProviderFailureCode;
  readonly retryable: boolean;
  readonly status?: number;

  constructor(input: {
    provider_name: string;
    code: ProviderFailureCode;
    message: string;
    retryable: boolean;
    status?: number;
  }) {
    super(input.message);
    this.name = 'ProviderRequestError';
    this.provider_name = input.provider_name;
    this.code = input.code;
    this.retryable = input.retryable;
    this.status = input.status;
  }
}

export function classifyProviderFailure(input: {
  provider_name: string;
  message: string;
  status?: number;
}): ProviderRequestError {
  const normalizedMessage = input.message.toLowerCase();

  if (input.status === 401 || input.status === 403 || normalizedMessage.includes('authentication failed')) {
    return new ProviderRequestError({
      provider_name: input.provider_name,
      code: 'provider_auth_invalid',
      message: input.message,
      retryable: false,
      status: input.status,
    });
  }

  if (input.status === 429 || normalizedMessage.includes('rate limit')) {
    return new ProviderRequestError({
      provider_name: input.provider_name,
      code: 'rate_limited',
      message: input.message,
      retryable: true,
      status: input.status,
    });
  }

  if (input.status === 408 || normalizedMessage.includes('timeout') || normalizedMessage.includes('timed out')) {
    return new ProviderRequestError({
      provider_name: input.provider_name,
      code: 'timeout',
      message: input.message,
      retryable: true,
      status: input.status,
    });
  }

  if (normalizedMessage.includes('revoked') || normalizedMessage.includes('revocation')) {
    return new ProviderRequestError({
      provider_name: input.provider_name,
      code: 'tls_cert_revoked',
      message: input.message,
      retryable: false,
      status: input.status,
    });
  }

  if (
    normalizedMessage.includes('certificate')
    || normalizedMessage.includes('ssl')
    || normalizedMessage.includes('tls')
    || normalizedMessage.includes('x509')
  ) {
    return new ProviderRequestError({
      provider_name: input.provider_name,
      code: 'tls_handshake_failed',
      message: input.message,
      retryable: false,
      status: input.status,
    });
  }

  if (
    normalizedMessage.includes('proxy')
    && (
      normalizedMessage.includes('econnrefused')
      || input.status === 407
      || normalizedMessage.includes('407')
      || normalizedMessage.includes('proxy connect')
    )
  ) {
    return new ProviderRequestError({
      provider_name: input.provider_name,
      code: 'proxy_rejected',
      message: input.message,
      retryable: true,
      status: input.status,
    });
  }

  if (
    normalizedMessage.includes('socket hang up')
    || normalizedMessage.includes('broken pipe')
    || normalizedMessage.includes('connection closed')
    || normalizedMessage.includes('connection aborted')
    || normalizedMessage.includes('econnreset')
    || normalizedMessage.includes('unexpected eof')
    || normalizedMessage.includes('stream terminated unexpectedly')
  ) {
    return new ProviderRequestError({
      provider_name: input.provider_name,
      code: 'connection_interrupted',
      message: input.message,
      retryable: true,
      status: input.status,
    });
  }

  if (
    normalizedMessage.includes('enotfound')
    || normalizedMessage.includes('econnrefused')
    || normalizedMessage.includes('econnreset')
    || normalizedMessage.includes('network is unreachable')
    || normalizedMessage.includes('dns')
    || normalizedMessage.includes('getaddrinfo')
  ) {
    return new ProviderRequestError({
      provider_name: input.provider_name,
      code: 'network_unreachable',
      message: input.message,
      retryable: true,
      status: input.status,
    });
  }

  if (
    (input.status !== undefined && input.status >= 500 && input.status <= 599)
    || normalizedMessage.includes('service unavailable')
    || normalizedMessage.includes('bad gateway')
    || normalizedMessage.includes('gateway timeout')
  ) {
    return new ProviderRequestError({
      provider_name: input.provider_name,
      code: 'provider_upstream_failed',
      message: input.message,
      retryable: true,
      status: input.status,
    });
  }

  return new ProviderRequestError({
    provider_name: input.provider_name,
    code: 'provider_unavailable',
    message: input.message,
    retryable: input.status === undefined || input.status >= 500,
    status: input.status,
  });
}

/** Provider is a chat-capable model backend. */
export interface Provider {
  name(): string;
  /** Start a streaming completion, yielding chunks. Cancelling signal aborts. */
  stream(signal: AbortSignal, req: Request): AsyncGenerator<Chunk>;
}

// ---- Tool pairing sanitization ----

const interruptedToolResult =
  '[no result: the previous turn was interrupted before this tool call completed]';

/** Repair history so every assistant tool_calls has matching tool messages. */
export function sanitizeToolPairing(msgs: Message[]): Message[] {
  const out: Message[] = [];
  let i = 0;
  while (i < msgs.length) {
    const m = msgs[i];
    if (m.role === 'assistant' && m.tool_calls && m.tool_calls.length > 0) {
      let j = i + 1;
      while (j < msgs.length && msgs[j].role === 'tool') j++;
      out.push(m);
      out.push(...pairToolResults(m.tool_calls, msgs.slice(i + 1, j)));
      i = j;
      continue;
    }
    if (m.role === 'tool') {
      i++; // orphan tool message — drop
      continue;
    }
    // Skip empty assistant messages — DeepSeek rejects them
    if (m.role === 'assistant' && !m.content && (!m.tool_calls || m.tool_calls.length === 0)) {
      i++;
      continue;
    }
    out.push(m);
    i++;
  }
  return out;
}

function pairToolResults(calls: ToolCall[], available: Message[]): Message[] {
  return calls.map((tc) => {
    const found = available.find((r) => r.tool_call_id === tc.id);
    if (found) return found;
    return {
      role: 'tool' as Role,
      tool_call_id: tc.id,
      name: tc.name,
      content: interruptedToolResult,
    };
  });
}
