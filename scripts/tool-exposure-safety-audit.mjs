#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { repoRoot, deriveProjectName, deriveProjectVersion } from './lib/project-metadata.mjs';

function read(relative) {
  return fs.readFileSync(path.join(repoRoot, relative), 'utf8');
}

function exists(relative) {
  return fs.existsSync(path.join(repoRoot, relative));
}

function check(id, ok, detail, severity = 'blocker') {
  return { id, status: ok ? 'pass' : 'fail', severity: ok ? 'info' : severity, detail };
}

export function collectToolExposureSafetyAudit() {
  const adapter = read('src/adapter.rs');
  const leaseRuntime = read('src/upstream/lease_runtime.rs');
  const policyAudit = read('src/upstream/policy_audit.rs');
  const mcpSurface = read('src/mcp_server/tool_surface.rs');
  const httpSurface = read('src/dashboard/http_tools.rs');
  const dynamicAdapter = read('docs/dynamic-adapter.md');
  const universalAdapter = read('docs/universal-dynamic-adapter.md');
  const toolSafetyDocExists = exists('docs/tool-exposure-and-call-safety.md');
  const toolSafetyDoc = toolSafetyDocExists ? read('docs/tool-exposure-and-call-safety.md') : '';
  const checks = [
    check(
      'broker-default',
      /fn default_tool_exposure_mode\(\) -> ToolExposureMode \{\n\s*ToolExposureMode::Broker\n\}/.test(adapter),
      'Default adapter exposure is broker-first, not native projection-first.'
    ),
    check(
      'safe-projection-default',
      /const DEFAULT_PROJECTED_TOOL_SAFETY:\s*ProjectionSafety\s*=\s*ProjectionSafety::Safe/.test(adapter),
      'Opt-in native projection defaults to safe projection only.'
    ),
    check(
      'known-tool-call-guard',
      /fn validate_upstream_tool_known\(/.test(leaseRuntime)
        && (leaseRuntime.match(/validate_upstream_tool_known\(/g) || []).length >= 3
        && /fn validate_upstream_batch_tools_known\(/.test(leaseRuntime),
      'Brokered upstream calls validate the requested tool against current tools/list before forwarding.'
    ),
    check(
      'unknown-tool-escape-hatch-explicit',
      /MCPACE_ALLOW_UNKNOWN_UPSTREAM_TOOLS/.test(leaseRuntime)
        && /ALLOW_UNKNOWN_TOOL_ARGUMENT/.test(leaseRuntime)
        && /allowUnknownTool/.test(mcpSurface)
        && /allowUnknownTool/.test(httpSurface),
      'Dynamic/hidden upstream tools require an explicit allowUnknownTool or operator-level escape hatch.'
    ),
    check(
      'metadata-injection-risk-detected',
      /metadata-injection/.test(policyAudit)
        && /ignore previous/.test(policyAudit)
        && /system prompt/.test(policyAudit)
        && /risk_class_recommends_policy/.test(policyAudit),
      'Tool title/description metadata injection patterns are policy-advisory risk signals.'
    ),
    check(
      'projection-control-args-forwarded',
      /allowUnknownTool/.test(adapter) && /allowUnknownUpstreamTool/.test(adapter),
      'Projected tool control arguments can pass explicit unknown-tool opt-in to brokered calls.'
    ),
    check(
      'safety-docs-present',
      toolSafetyDocExists
        && /Known-tool call guard/i.test(toolSafetyDoc)
        && /metadata-injection/i.test(toolSafetyDoc)
        && /broker-first/i.test(toolSafetyDoc),
      'Tool exposure/call safety lifecycle is documented.'
    ),
    check(
      'docs-updated',
      /projection safety default is `safe`/i.test(dynamicAdapter)
        && /projection safety default is `safe`/i.test(universalAdapter)
        && /tool-exposure-and-call-safety\.md/.test(read('docs/README.md')),
      'Adapter docs describe safe projection defaults and link to the tool safety contract.'
    ),
  ];
  const blockers = checks.filter((item) => item.status !== 'pass' && item.severity === 'blocker');
  return {
    schema: 'mcpace.toolExposureSafetyAudit.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status: blockers.length ? 'fail' : 'pass',
    summary: { checks: checks.length, blockers: blockers.length },
    checks,
  };
}

function parseArgs(argv) {
  const parsed = { json: false, write: null, strict: false, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = argv[++index] || 'reports/tool-exposure-safety-latest.json'; break;
      case '--strict': parsed.strict = true; break;
      case '-h':
      case '--help': parsed.help = true; break;
      default: throw new Error(`unsupported tool-exposure-safety-audit argument: ${token}`);
    }
  }
  return parsed;
}

function writeJson(filePath, report) {
  const target = path.resolve(filePath);
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
      process.stdout.write('Usage: node scripts/tool-exposure-safety-audit.mjs [--json] [--write <path>] [--strict]\n');
      return;
    }
    const report = collectToolExposureSafetyAudit();
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
