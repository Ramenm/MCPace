import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

function readText(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

test('import code accepts common MCP URL aliases and normalizes remote type', () => {
  const source = readText('src/mcp_sources/import.rs');
  for (const key of ['serverUrl', 'httpUrl', 'endpoint']) {
    assert.match(source, new RegExp(`"${key}"`), `missing URL alias support for ${key}`);
  }
  assert.match(source, /source_type::infer_public_source_type/, 'remote URL imports should use the shared source-type normalizer');
  assert.match(source, /"servers"/, 'VS Code-style top-level servers object should remain supported');
  assert.match(source, /"mcpServers"/, 'mcpServers object should remain supported');
});

test('home import does not add default upstream servers and skips MCPace loops', () => {
  const setup = readText('src/setup.rs');
  assert.match(setup, /did not add a default filesystem server/i);
  assert.match(setup, /auto-imported-home\.json/);
  assert.match(setup, /normalized_name == "mcpace" \|\| normalized_name == "mcp-pace"/);
  assert.doesNotMatch(setup, /normalized_name == "mcp-ace"/);
});

test('public help stays compact and install type inference remains documented', () => {
  const app = readText('src/app.rs');
  const help = app.slice(app.indexOf('fn write_help'));
  const usageLines = [...help.matchAll(/writeln!\(stdout, "  mcpace /g)].length;
  assert.ok(usageLines <= 8, `help should keep visible commands compact, saw ${usageLines} mcpace lines`);
  assert.match(help, /Server type is inferred/);
  assert.match(help, /It does not add a default upstream server/);
});


test('HTTP path normalization rejects request-line injection primitives', () => {
  const runtimePaths = readText('src/runtimepaths.rs');
  const runtimePathTests = readText('src/runtimepaths/tests.rs');
  assert.match(runtimePaths, /trimmed\s*\.chars\(\)\s*\.any\(\|ch\| ch\.is_control\(\) \|\| ch\.is_whitespace\(\)\)/s, 'normalized HTTP paths must reject all whitespace/control characters, not only CRLF');
  assert.match(runtimePathTests, /normalize_http_path_rejects_request_line_injection_primitives/, 'Rust regression test must cover HTTP request-line injection primitives');
  assert.match(runtimePathTests, /"\/mcp with-space"/, 'space-containing request paths must be covered');
  assert.match(runtimePathTests, /"\/mcp\\twith-tab"/, 'tab-containing request paths must be covered');
});


test('public MCP URL normalization rejects invalid URL text before export', () => {
  const runtimePaths = readText('src/runtimepaths.rs');
  const runtimePathTests = readText('src/runtimepaths/tests.rs');
  assert.match(runtimePaths, /fn normalize_public_url\(value: &str\) -> Option<String>/);
  assert.match(runtimePaths, /trimmed\s*\.chars\(\)\s*\.any\(\|ch\| ch\.is_control\(\) \|\| ch\.is_whitespace\(\)\)/s, 'public URL normalization must reject all whitespace/control characters');
  assert.match(runtimePathTests, /normalize_public_url_rejects_ambiguous_or_unsafe_authorities/, 'Rust regression test must cover invalid public URL text and unsafe authorities');
  assert.match(runtimePaths, /valid_public_url_authority/, 'public URL export must validate authority, not only scheme');
  assert.match(runtimePaths, /authority\.contains\('@'\)/, 'public URL export must reject userinfo authority confusion');
  assert.match(runtimePaths, /authority\.matches\(':'\)\.count\(\) > 1/, 'public URL export must reject raw IPv6 authorities');
});

test('MCP settings writers preserve either mcpServers or servers top-level shape', () => {
  const writer = readText('src/mcp_sources/write.rs');
  const importer = readText('src/mcp_sources/import.rs');

  assert.match(writer, /fn ensure_servers_object_mut/);
  assert.match(writer, /fn existing_servers_object_mut/);
  assert.match(writer, /root_object\.contains_key\("mcpServers"\)[\s\S]*root_object\.contains_key\("servers"\)/);
  assert.match(writer, /has no mcpServers or servers object/);
  assert.doesNotMatch(writer, /root_object\.get_mut\("mcpServers"\)/);

  assert.match(importer, /json_helpers::mcp_servers_object\(value\)/);
  assert.match(importer, /fn ensure_import_servers_object_mut/);
  assert.match(importer, /root_object\.contains_key\("mcpServers"\)[\s\S]*root_object\.contains_key\("servers"\)/);
  assert.doesNotMatch(importer, /root_object\.get_mut\("mcpServers"\)/);
});

test('MCPace self-loop import detection requires an endpoint path boundary', () => {
  const importer = readText('src/mcp_sources/import.rs');
  const selfEntry = importer.slice(importer.indexOf('fn looks_like_mcpace_self_entry'), importer.indexOf('fn matches_endpoint_url'));
  const matcher = importer.slice(importer.indexOf('fn matches_endpoint_url'), importer.indexOf('fn read_or_new_settings'));

  assert.match(selfEntry, /matches_endpoint_url\(&url, &configured_url\)/);
  assert.match(selfEntry, /matches_endpoint_url\(&url, "http:\/\/127\.0\.0\.1:39022\/mcp"\)/);
  assert.doesNotMatch(selfEntry, /url\.starts_with\("http:\/\/127\.0\.0\.1:39022\/mcp"\)/);
  assert.match(matcher, /strip_prefix\(endpoint\)/);
  assert.match(matcher, /matches!\(suffix\.as_bytes\(\)\.first\(\), Some\(b'\/' \| b'\?' \| b'#'\)\)/);
});

test('client install config patching is centralized behind a typed edit boundary', () => {
  const configEdit = readText('src/config_edit.rs');
  const updater = readText('src/client/actions/config_update.rs');
  const actions = readText('src/client/actions.rs');

  assert.match(configEdit, /enum ConfigEditError/);
  assert.match(configEdit, /impl std::error::Error for ConfigEditError/);
  assert.match(configEdit, /pub\(crate\) fn apply_json_mcp_server_entry/);
  assert.match(configEdit, /pub\(crate\) fn apply_toml_mcp_server_block/);
  assert.match(configEdit, /pub\(crate\) fn apply_yaml_mcp_server_entry/);
  assert.equal([...configEdit.matchAll(/"enabled = true"\.to_string\(\)/g)].length, 1, 'TOML managed block must not duplicate enabled=true');
  assert.doesNotMatch(updater, /upsert_toml|upsert_yaml|parse_yaml|find_toml/, 'config_update.rs should only own diff rendering now');
  assert.match(actions, /\.map_err\(\|error\| error\.to_string\(\)\)\?/, 'client boundary should render typed edit errors at the CLI layer');
});
