const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..', '..');

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath));
}

function lineCount(relativePath) {
  return read(relativePath).trimEnd().split(/\r?\n/).length;
}

test('toolchain support policy stays aligned across manifests, local version files, and CI', () => {
  const policy = readJson(path.join('reports', 'toolchain-support.json'));
  const pkg = readJson('package.json');
  const cliPkg = readJson(path.join('packages', 'npm', 'cli', 'package.json'));
  const workflow = read(path.join('.github', 'workflows', 'ci.yml'));
  const rustToolchain = read('rust-toolchain.toml');
  const nvmrc = read('.nvmrc').trim();
  const nodeVersion = read('.node-version').trim();
  const toolchainDoc = read(path.join('docs', 'toolchain-policy.md'));
  const hostSetup = read(path.join('docs', 'host-setup.md'));

  assert.equal(pkg.engines.node, policy.node.enginesRange);
  assert.equal(pkg.engines.npm, `>=${policy.node.minimumNpm}`);
  assert.equal(pkg.packageManager, policy.packageManager);
  assert.equal(pkg.devEngines.runtime.version, policy.node.devEnginesRuntimeRange);
  assert.equal(pkg.devEngines.packageManager.version, policy.node.devEnginesNpmRange);
  assert.equal(cliPkg.engines.node, policy.node.enginesRange);

  assert.equal(nvmrc, String(policy.node.defaultMajor));
  assert.equal(nodeVersion, String(policy.node.defaultMajor));

  assert.match(rustToolchain, new RegExp(`channel\\s*=\\s*"${policy.rust.toolchain.replace(/\./g, '\\.') }"`));
  assert.match(rustToolchain, new RegExp(`profile\\s*=\\s*"${policy.rust.profile}"`));
  assert.match(rustToolchain, /components\s*=\s*\["clippy",\s*"rustfmt"\]/);

  assert.match(workflow, new RegExp(`actions/checkout@${policy.githubActions.checkout}`));
  assert.match(workflow, new RegExp(`actions/setup-node@${policy.githubActions.setupNode}`));
  assert.match(workflow, /concurrency:/);
  assert.match(workflow, /package-dry-run:/);
  assert.match(workflow, /node-version-file:\s*\.nvmrc/);
  assert.match(workflow, new RegExp(`toolchain:\\s*${policy.rust.toolchain.replace(/\./g, '\\.')}`));

  for (const entry of policy.ci.nodeSourceValidationMatrix) {
    const snippet = `- os: ${entry.os}\n            node: '${entry.node}'`;
    assert.ok(workflow.includes(snippet), `workflow should include ${snippet}`);
  }
  for (const os of policy.ci.rustHosts) {
    assert.ok(
      workflow.includes(`- ${os}`) || workflow.includes(`runs-on: ${os}`),
      `workflow should include Rust host ${os}`
    );
  }

  for (const major of policy.node.supportedLtsMajors) {
    assert.match(toolchainDoc, new RegExp(`Node ${major}`));
    assert.match(hostSetup, new RegExp(`Node\\.js ${major}`));
  }
  assert.match(toolchainDoc, /\.nvmrc/);
  assert.match(toolchainDoc, /\.node-version/);
  assert.match(toolchainDoc, /machine-readable source of truth/i);
  assert.match(hostSetup, /\.nvmrc/);
  assert.match(hostSetup, /\.node-version/);
});

test('dependabot watches actions, npm, and cargo metadata weekly', () => {
  const dependabot = read(path.join('.github', 'dependabot.yml'));
  assert.match(dependabot, /package-ecosystem: github-actions/);
  assert.match(dependabot, /package-ecosystem: npm/);
  assert.match(dependabot, /package-ecosystem: cargo/);
  assert.match(dependabot, /interval: weekly/);
});

test('large command families keep thin module roots and split internals', () => {
  const roots = {
    client: read(path.join('src', 'client.rs')),
    hub: read(path.join('src', 'hub.rs')),
    lab: read(path.join('src', 'lab.rs')),
    server: read(path.join('src', 'server.rs')),
    verify: read(path.join('src', 'verify.rs'))
  };

  assert.match(roots.client, /mod actions;/);
  assert.match(roots.client, /mod args;/);
  assert.match(roots.client, /mod context;/);
  assert.match(roots.client, /mod metadata;/);
  assert.match(roots.client, /mod model;/);
  assert.match(roots.client, /mod pathing;/);
  assert.match(roots.client, /mod plan;/);
  assert.match(roots.client, /mod render;/);
  assert.match(roots.hub, /mod args;/);
  assert.match(roots.hub, /mod lifecycle;/);
  assert.match(roots.hub, /mod model;/);
  assert.match(roots.hub, /mod runtime;/);
  assert.match(roots.hub, /mod status;/);
  assert.match(roots.lab, /mod analysis;/);
  assert.match(roots.lab, /mod args;/);
  assert.match(roots.lab, /mod loader;/);
  assert.match(roots.lab, /mod model;/);
  assert.match(roots.lab, /mod render;/);
  assert.match(roots.server, /mod args;/);
  assert.match(roots.server, /mod loader;/);
  assert.match(roots.server, /mod model;/);
  assert.match(roots.server, /mod render;/);
  assert.match(roots.verify, /mod args;/);
  assert.match(roots.verify, /mod model;/);
  assert.match(roots.verify, /mod render;/);

  for (const name of readJson(path.join('reports', 'toolchain-support.json')).architecture.thinModuleRoots) {
    assert.ok(lineCount(path.join('src', `${name}.rs`)) < 100, `src/${name}.rs should stay thin`);
  }

  for (const relativePath of [
    path.join('src', 'client', 'actions.rs'),
    path.join('src', 'client', 'args.rs'),
    path.join('src', 'client', 'context.rs'),
    path.join('src', 'client', 'metadata.rs'),
    path.join('src', 'client', 'model.rs'),
    path.join('src', 'client', 'pathing.rs'),
    path.join('src', 'client', 'plan.rs'),
    path.join('src', 'client', 'render.rs'),
    path.join('src', 'hub', 'args.rs'),
    path.join('src', 'hub', 'lifecycle.rs'),
    path.join('src', 'hub', 'model.rs'),
    path.join('src', 'hub', 'runtime.rs'),
    path.join('src', 'hub', 'status.rs'),
    path.join('src', 'lab', 'analysis.rs'),
    path.join('src', 'lab', 'args.rs'),
    path.join('src', 'lab', 'loader.rs'),
    path.join('src', 'lab', 'model.rs'),
    path.join('src', 'lab', 'render.rs'),
    path.join('src', 'server', 'args.rs'),
    path.join('src', 'server', 'loader.rs'),
    path.join('src', 'server', 'model.rs'),
    path.join('src', 'server', 'render.rs'),
    path.join('src', 'verify', 'args.rs'),
    path.join('src', 'verify', 'model.rs'),
    path.join('src', 'verify', 'render.rs')
  ]) {
    assert.equal(fs.existsSync(path.join(repoRoot, relativePath)), true, `${relativePath} should exist`);
  }
});
