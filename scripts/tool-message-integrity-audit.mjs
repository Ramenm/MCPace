#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');

function read(rel) {
  return fs.readFileSync(path.join(repoRoot, rel), 'utf8');
}

function check(id, ok, detail) {
  return { id, status: ok ? 'pass' : 'fail', detail };
}

export function collectToolMessageIntegrityAudit() {
  const protocol = read('src/mcp_protocol.rs');
  const stdio = read('src/mcp_server.rs');
  const http = read('src/dashboard/mcp_http.rs');
  const httpRuntime = read('src/dashboard/tool_runtime.rs');
  const pkg = JSON.parse(read('package.json'));

  const checks = [
    check(
      'jsonrpc-envelope-validator-exists',
      /pub fn validate_request_envelope\(message: &JsonValue\) -> Result<\(\), String>/.test(protocol),
      'A shared JSON-RPC envelope validator exists instead of ad-hoc per-route checks.'
    ),
    check(
      'jsonrpc-version-required',
      /jsonrpc[\s\S]*JSONRPC_VERSION/.test(protocol) && /must declare jsonrpc/.test(protocol),
      'Requests must declare jsonrpc="2.0" before dispatch.'
    ),
    check(
      'jsonrpc-method-type-required',
      /JSON-RPC method must be a string/.test(protocol) && /JSON-RPC method must be non-empty/.test(protocol),
      'The method field is type-checked and cannot be empty.'
    ),
    check(
      'jsonrpc-id-type-checked',
      /JSON-RPC id must be a string, number, or null/.test(protocol),
      'Request IDs are restricted to JSON-RPC-compatible primitive forms.'
    ),
    check(
      'jsonrpc-params-type-checked',
      /JSON-RPC params must be an object, array, or null/.test(protocol),
      'Params are rejected when they are scalar values that cannot represent MCP request payloads.'
    ),
    check(
      'stdio-validates-envelope-before-dispatch',
      /validate_request_envelope\(&message\)[\s\S]*let method = json_helpers::string_at_path/.test(stdio),
      'stdio MCP server validates JSON-RPC envelope before matching methods.'
    ),
    check(
      'http-validates-envelope-before-dispatch',
      /validate_request_envelope\(&message\)[\s\S]*validate_mcp_standard_headers/.test(http),
      'Streamable HTTP route validates JSON-RPC envelope before dispatch/header contract handling.'
    ),
    check(
      'top-level-tool-arguments-object-stdio',
      /tool_call_arguments_or_empty\(&message\)/.test(stdio),
      'stdio tools/call rejects non-object params.arguments.'
    ),
    check(
      'top-level-tool-arguments-object-http',
      /tool_call_arguments_or_empty\(&message\)/.test(http),
      'HTTP tools/call rejects non-object params.arguments.'
    ),
    check(
      'prompt-arguments-object-http',
      /params_arguments_object_or_empty\(&message, "prompts\/get"\)/.test(http),
      'HTTP prompts/get also rejects scalar arguments instead of forwarding malformed payloads.'
    ),
    check(
      'nested-upstream-call-arguments-object-stdio',
      /optional_object_argument\(arguments, "arguments"\)\?/.test(stdio),
      'Brokered stdio upstream_call requires its nested arguments field to be an object.'
    ),
    check(
      'nested-upstream-call-arguments-object-http',
      /optional_object_arg\(args, "arguments"\)\?/.test(httpRuntime),
      'Brokered HTTP upstream_call requires its nested arguments field to be an object.'
    ),
    check(
      'batch-tuple-arguments-object-stdio',
      stdio.includes('calls[{}][1] must be a JSON object when present')
        || /calls\[\{\}\]\[1\]/.test(stdio),
      'Tuple batch calls reject scalar second elements.'
    ),
    check(
      'batch-object-arguments-object-stdio',
      stdio.includes('upstream_batch calls[{}].arguments must be a JSON object')
        || /upstream_batch calls\[\{\}\]\.arguments/.test(stdio),
      'Object-form batch calls reject scalar arguments.'
    ),
    check(
      'batch-tuple-arguments-object-http',
      httpRuntime.includes('upstream_batch calls[{}][1] must be a JSON object when present')
        || /upstream_batch calls\[\{\}\]\[1\]/.test(httpRuntime),
      'HTTP tuple batch calls reject scalar second elements.'
    ),
    check(
      'batch-object-arguments-object-http',
      /optional_object_arg\(raw_call, "arguments"\)\?/.test(httpRuntime),
      'HTTP object-form batch calls reject scalar arguments.'
    ),
    check(
      'message-integrity-script-registered',
      pkg.scripts?.['verify:tool-message-integrity'] === 'node scripts/tool-message-integrity-audit.mjs --json --write reports/tool-message-integrity-latest.json --strict',
      'The message-integrity audit is available as an npm verification command.'
    )
  ];

  const failures = checks.filter((item) => item.status !== 'pass');
  return {
    schema: 'mcpace.toolMessageIntegrityAudit.v1',
    generatedAt: new Date().toISOString(),
    status: failures.length ? 'fail' : 'pass',
    summary: { checks: checks.length, failures: failures.length },
    checks
  };
}

function parseArgs(argv) {
  const parsed = { json: false, write: null, strict: false, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (token === '--json') parsed.json = true;
    else if (token === '--write') parsed.write = argv[++index] || 'reports/tool-message-integrity-latest.json';
    else if (token === '--strict') parsed.strict = true;
    else if (token === '-h' || token === '--help') parsed.help = true;
    else throw new Error(`unsupported tool-message-integrity argument: ${token}`);
  }
  return parsed;
}

function writeJson(filePath, report) {
  const target = path.resolve(repoRoot, filePath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    if (parsed.help) {
      process.stdout.write('Usage: node scripts/tool-message-integrity-audit.mjs [--json] [--write <path>] [--strict]\n');
      return;
    }
    const report = collectToolMessageIntegrityAudit();
    if (parsed.write) writeJson(parsed.write, report);
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else process.stdout.write(`${report.status}\n`);
    if (parsed.strict && report.status !== 'pass') process.exit(1);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
