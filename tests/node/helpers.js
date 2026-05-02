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

const SAFE_CHILD_ENV_KEYS = [
  'PATH',
  'Path',
  'HOME',
  'USER',
  'LOGNAME',
  'SHELL',
  'SystemRoot',
  'WINDIR',
  'COMSPEC',
  'PATHEXT',
  'TEMP',
  'TMP',
  'TMPDIR',
  'USERPROFILE',
  'APPDATA',
  'LOCALAPPDATA',
  'PROGRAMDATA',
  'GITHUB_ACTIONS',
  'GITHUB_WORKSPACE',
  'RUNNER_OS',
  'RUNNER_TEMP',
  'RUNNER_TOOL_CACHE',
  'NODE_EXTRA_CA_CERTS',
  'SSL_CERT_FILE',
  'CARGO_HOME',
  'RUSTUP_HOME',
  'DOCKER_HOST',
  'DOCKER_CONTEXT',
  'DOCKER_TLS_VERIFY',
  'DOCKER_CERT_PATH'
];

function cleanChildEnv(overrides = {}) {
  const env = {};
  for (const key of SAFE_CHILD_ENV_KEYS) {
    if (process.env[key]) {
      env[key] = process.env[key];
    }
  }
  for (const [key, value] of Object.entries(overrides)) {
    if (value === undefined || value === null) {
      delete env[key];
    } else {
      env[key] = value;
    }
  }
  return env;
}

module.exports = {
  repoRoot,
  read,
  readJson,
  packageVersion,
  cleanChildEnv
};
