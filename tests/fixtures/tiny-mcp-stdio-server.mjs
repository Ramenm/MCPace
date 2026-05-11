#!/usr/bin/env node
import process from 'node:process';

const tools = [
  {
    name: 'tiny_echo',
    description: 'Deterministic echo tool used by MCPace runtime trace smoke tests.',
    inputSchema: {
      type: 'object',
      properties: {
        message: { type: 'string' }
      },
      required: ['message']
    }
  }
];

function writeJson(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function result(id, value) {
  writeJson({ jsonrpc: '2.0', id, result: value });
}

function error(id, code, message) {
  writeJson({ jsonrpc: '2.0', id, error: { code, message } });
}

function handleRequest(request) {
  if (!request || request.jsonrpc !== '2.0') {
    error(request?.id ?? null, -32600, 'invalid JSON-RPC request');
    return;
  }

  if (request.method === 'notifications/initialized') {
    return;
  }

  switch (request.method) {
    case 'initialize':
      result(request.id, {
        protocolVersion: request.params?.protocolVersion || '2025-03-26',
        capabilities: { tools: {} },
        serverInfo: { name: 'mcpace-tiny-stdio-fixture', version: '0.1.0' }
      });
      break;
    case 'tools/list':
      result(request.id, { tools });
      break;
    case 'tools/call': {
      const name = request.params?.name;
      if (name !== 'tiny_echo') {
        error(request.id, -32602, `unknown tool: ${name || '<missing>'}`);
        return;
      }
      const message = String(request.params?.arguments?.message ?? '');
      result(request.id, {
        content: [{ type: 'text', text: `tiny_echo:${message}` }],
        isError: false
      });
      break;
    }
    default:
      error(request.id, -32601, `method not found: ${request.method || '<missing>'}`);
      break;
  }
}

let buffer = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk) => {
  buffer += chunk;
  let newlineIndex;
  while ((newlineIndex = buffer.indexOf('\n')) >= 0) {
    const line = buffer.slice(0, newlineIndex).trim();
    buffer = buffer.slice(newlineIndex + 1);
    if (!line) continue;
    try {
      handleRequest(JSON.parse(line));
    } catch (err) {
      error(null, -32700, `parse error: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
});
