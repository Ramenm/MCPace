import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

const read = (relativePath) => fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');

test('MCP settings writes use exclusive locks, private atomic writes, and cross-source shadow checks', () => {
  const runtimePaths = read('src/runtimepaths.rs');
  assert.match(runtimePaths, /pub fn write_private_text_atomic\(/);
  assert.match(runtimePaths, /fn write_text_atomic_with_mode\(/);
  assert.match(runtimePaths, /pub fn acquire_exclusive_file_lock\(/);
  assert.match(runtimePaths, /create_new\(true\)/);
  assert.match(runtimePaths, /remove_file\(&self\.path\)/);

  const writer = read('src/mcp_sources/write.rs');
  assert.match(writer, /reject_cross_source_shadowing\(/);
  assert.match(writer, /acquire_mcp_settings_namespace_lock\(root_path\)/);
  assert.match(writer, /acquire_exclusive_file_lock\([^)]*MCP settings update/s);
  assert.match(writer, /write_private_text_atomic\(&target_path/);
  assert.match(writer, /read_settings_or_empty\(&target_path\)/);
  assert.match(writer, /read_existing_settings\(&target_path\)/);
  assert.doesNotMatch(writer, /target_path\.is_file\(\)/);
  assert.doesNotMatch(writer, /write_text_atomic\(&target_path/);

  const importer = read('src/mcp_sources/import.rs');
  assert.match(importer, /acquire_mcp_settings_namespace_lock\(root_path\)/);
  assert.match(importer, /acquire_exclusive_file_locks\([^)]*MCP settings import/s);
  assert.match(importer, /source_paths_for_normalized_server\(/);
  assert.match(importer, /write_private_text_atomic\(target_path/);
  assert.match(importer, /symlink_metadata\(path\)/);
  assert.doesNotMatch(importer, /write_text_atomic\(target_path/);
});

test('MCP source loading avoids symlink/non-regular sources and uses content fingerprints', () => {
  const sources = read('src/mcp_sources.rs');
  assert.match(sources, /fn settings_source_is_regular_file\(/);
  assert.match(sources, /file_type\(\)\.is_symlink\(\)/);
  assert.match(sources, /pub\(crate\) fn acquire_mcp_settings_namespace_lock\(/);
  assert.match(sources, /mcp-settings\.namespace/);
  assert.match(sources, /pub\(crate\) fn source_paths_for_normalized_server\(/);
  assert.match(sources, /feed_settings_file_fingerprint\(&source\.path, &mut fingerprint\)/);
  assert.match(sources, /\.take\(MAX_MCP_SETTINGS_FINGERPRINT_BYTES\.saturating_add\(1\)\)/);
  assert.match(sources, /reader\.read\(&mut buffer\)/);
  assert.doesNotMatch(sources, /duration_since\(UNIX_EPOCH\)/);

  const sourcePaths = read('src/mcp_sources/paths.rs');
  assert.match(sourcePaths, /fs::symlink_metadata\(directory\)/);
  assert.match(sourcePaths, /file_type\(\)\s*\n\s*\.map\(\|file_type\| file_type\.is_file\(\) && !file_type\.is_symlink\(\)\)/);
  assert.doesNotMatch(sourcePaths, /directory\.exists\(\)/);
  assert.match(sourcePaths, /MAX_MCP_SETTINGS_FILES_PER_DIRECTORY/);
});

test('MCP discovery and registry refresh harden endpoint normalization, cache writes, and duplicate selection', () => {
  const discover = read('src/server/discover.rs');
  assert.match(discover, /fn normalize_registry_endpoint\(/);
  assert.match(discover, /starts_with\("https:\/\/"\)/);
  assert.match(discover, /authority\.contains\('@'\)/);
  assert.match(discover, /acquire_exclusive_file_lock\([^)]*MCP registry cache refresh/s);
  assert.match(discover, /write_private_text_atomic\(&cache_path/);
  assert.match(discover, /fn deduplicate_discovery_candidates\(/);
  assert.match(discover, /candidate_trust_rank\(/);
  assert.match(discover, /http_client::bounded_get_text\(/);
  assert.match(discover, /MAX_REGISTRY_PAGES/);
  assert.match(discover, /8 \* 1024 \* 1024/);
  assert.doesNotMatch(discover, /Command::new\(/);
  const httpClient = read('src/http_client.rs');
  assert.match(httpClient, /RootCerts::PlatformVerifier/);
  assert.match(httpClient, /max_redirects\(0\)/);
  assert.match(httpClient, /timeout_global\(Some\(timeout\)\)/);
});

test('MCP auto-install planning rejects shell/path/URL package identifier classes', () => {
  const autoinstall = read('src/mcp_autoinstall.rs');
  assert.match(autoinstall, /fn validate_install_identifier\(/);
  assert.match(autoinstall, /uses_shell_composition\(trimmed\)/);
  assert.match(autoinstall, /trimmed\.starts_with\('-'\)/);
  assert.match(autoinstall, /trimmed\.contains\(":\/\/"\)/);
  assert.match(autoinstall, /trimmed\.starts_with\('\/'\)/);
  assert.ok(autoinstall.includes('validate_install_identifier("npm"'));
  assert.match(autoinstall, /validate_install_identifier\(\s*"pypi"/s);
  assert.match(autoinstall, /validate_install_identifier\(\s*"oci"/s);
});

test('MCP server runtime loading honors disabled:true and limits plain HTTP to loopback', () => {
  const serverConfig = read('src/upstream/server_config.rs');
  assert.match(serverConfig, /fn source_enabled_from_mcp_settings\(/);
  assert.match(serverConfig, /bool_at_path\(raw, &\["disabled"\]\)\.unwrap_or\(false\)/);
  assert.match(serverConfig, /bool_at_path\(raw, &\["enabled"\]\)\.unwrap_or\(true\)/);

  const httpRuntime = read('src/upstream/http_runtime.rs');
  const httpRuntimeTests = read('src/upstream/http_runtime/tests.rs');
  assert.match(httpRuntime, /plain_http_upstream_host_is_loopback\(&host\)/);
  assert.match(httpRuntime, /direct plain-HTTP MCP upstreams are limited to loopback hosts/);
  assert.match(httpRuntime, /eq_ignore_ascii_case\("localhost"\)/);
  assert.match(httpRuntime, /parse::<std::net::IpAddr>\(\)/);
  assert.match(httpRuntime, /address\.is_loopback\(\)/);
  assert.doesNotMatch(httpRuntime, /starts_with\("127\."\)/);
  assert.match(httpRuntimeTests, /parse_http_url_rejects_non_loopback_plain_http_upstreams/);
});

test('release manifest ships MCP lifecycle hardening documentation', () => {
  const manifest = JSON.parse(read('release-manifest.json'));
  assert.ok(manifest.includePaths.includes('docs/mcp-lifecycle-hardening.md'));
});
