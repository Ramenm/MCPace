import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

const read = (...parts) => fs.readFileSync(path.join(repoRoot, ...parts), 'utf8');
const readJson = (...parts) => JSON.parse(read(...parts));

test('server discover is wired as safe dynamic discovery, not blind execution', () => {
  const server = read('src', 'server.rs');
  const args = read('src', 'server', 'args.rs');
  const discover = read('src', 'server', 'discover.rs');
  const autoinstall = read('src', 'mcp_autoinstall.rs');
  const app = read('src', 'app.rs');

  assert.match(server, /mod\s+discover;/);
  assert.match(server, /action\s*==\s*"discover"\s*\|\|\s*action\s*==\s*"auto"/);
  assert.match(args, /"discover"/);
  assert.match(args, /"auto"/);
  assert.doesNotMatch(args, /long\s*=\s*"auto"/);
  assert.doesNotMatch(args, /auto_mode/);
  assert.match(args, /auto_install/);
  assert.match(app, /"server"\s*=>\s*server::run/);
  assert.doesNotMatch(app, /"autodiscover"/);
  assert.match(args, /allow_review_install/);
  assert.match(discover, /dynamic-server-discovery/);
  assert.match(discover, /registry\.modelcontextprotocol\.io/);
  assert.match(discover, /catalog\/registry-cache\.json/);
  assert.match(discover, /refresh_registry_cache/);
  assert.match(discover, /blocked-by-trust-policy/);
  assert.match(discover, /unknown or blocked candidates are never executed silently/);
  assert.match(discover, /selected_install_candidate/);
  assert.match(discover, /automatic_install_sweep/);
  assert.match(discover, /maxAutoInstallsPerRun/);
  assert.match(discover, /autoMode/);
  assert.match(discover, /registryCacheNeedsRefresh/);
  assert.match(discover, /registry_list_url_with_cursor/);
  assert.match(discover, /url_query_escape/);
  assert.match(discover, /postInstallProbeResults/);
  assert.ok(discover.includes('format!("npm:{}", identifier)'));
  assert.ok(discover.includes('format!("pypi:{}", identifier)'));
  assert.ok(discover.includes('format!("oci:{}", identifier)'));
  assert.ok(discover.includes('format!("nuget:{}", identifier)'));
  assert.ok(discover.includes('format!("mcpb:{}", identifier)'));
  assert.match(autoinstall, /pub\s+fn\s+plan_auto_install/);
  assert.match(autoinstall, /lower\.starts_with\(\"npm:\"\)/);
  assert.match(autoinstall, /lower\.starts_with\(\"nuget:\"\)/);
  assert.ok(autoinstall.includes("server install type '{}' is recognized from the MCP Registry but not executable"));
});

test('dynamic discovery config and schema expose safe auto-install controls', () => {
  const config = readJson('mcpace.config.json');
  const schema = readJson('schemas', 'mcpace-config.schema.json');
  const catalog = readJson('catalog', 'approved-servers.json');

  assert.equal(config.dynamicDiscovery.enabled, true);
  assert.equal(config.dynamicDiscovery.mode, 'auto');
  assert.equal(config.dynamicDiscovery.autoInstall, 'trusted-only');
  assert.equal(config.dynamicDiscovery.installUnknown, 'plan-only');
  assert.equal(config.dynamicDiscovery.maxAutoInstallsPerRun, 4);
  assert.equal(config.dynamicDiscovery.probeAfterInstall, true);
  assert.equal(config.dynamicDiscovery.autoRefreshRegistry, true);
  assert.equal(config.dynamicDiscovery.registryCacheTtlHours, 24);
  assert.equal(config.dynamicDiscovery.defaultCommand, 'auto');
  assert.equal(config.dynamicDiscovery.registryCachePath, './catalog/registry-cache.json');
  assert.ok(config.dynamicDiscovery.registryEndpoints.includes('https://registry.modelcontextprotocol.io'));

  assert.ok(schema.properties.dynamicDiscovery);
  assert.ok(schema.properties.dynamicDiscovery.properties.mode.enum.includes('auto'));
  assert.equal(schema.properties.dynamicDiscovery.properties.autoRefreshRegistry.type, 'boolean');
  assert.equal(schema.properties.dynamicDiscovery.properties.registryCacheTtlHours.type, 'integer');
  assert.deepEqual(schema.properties.dynamicDiscovery.properties.autoInstall.enum, ['off', 'trusted-only', 'review']);
  assert.deepEqual(schema.properties.dynamicDiscovery.properties.installUnknown.enum, ['never', 'plan-only']);
  assert.equal(schema.properties.dynamicDiscovery.properties.maxAutoInstallsPerRun.type, 'integer');

  assert.equal(catalog.servers.filesystem.trustLevel, 'approved');
  assert.equal(
    catalog.servers.filesystem.installSpec,
    'npm:@modelcontextprotocol/server-filesystem@2026.7.4',
  );
  assert.equal(catalog.servers.filesystem.type, 'stdio');
  assert.equal(catalog.servers.fetch.installSpec, 'pypi:mcp-server-fetch==2026.6.4');
});
