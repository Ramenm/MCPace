const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..', '..');

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath));
}

function packageVersion() {
  return readJson('package.json').version;
}

function cleanChildEnv(overrides = {}) {
  const env = { ...process.env, ...overrides };
  delete env.NODE_TEST_CONTEXT;
  return env;
}

module.exports = {
  repoRoot,
  read,
  readJson,
  packageVersion,
  cleanChildEnv
};
