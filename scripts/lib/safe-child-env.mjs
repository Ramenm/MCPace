import process from 'node:process';

export const SAFE_CHILD_ENV_KEYS = Object.freeze([
  'PATH',
  'Path',
  'PWD',
  'HOME',
  'USER',
  'LOGNAME',
  'HOSTNAME',
  'LANG',
  'LC_ALL',
  'LC_CTYPE',
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
  'RUSTFLAGS',
  'CARGO_INCREMENTAL',
  'CARGO_PROFILE_DEV_DEBUG',
  'CARGO_PROFILE_TEST_DEBUG',
  'CARGO_PROFILE_RELEASE_DEBUG',
  'DOCKER_HOST',
  'DOCKER_CONTEXT',
  'DOCKER_TLS_VERIFY',
  'DOCKER_CERT_PATH'
]);

export function cleanChildEnv(overrides = {}, baseEnv = process.env) {
  const env = {};
  for (const key of SAFE_CHILD_ENV_KEYS) {
    if (baseEnv[key]) {
      env[key] = baseEnv[key];
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

export function childEnvForCommand(_command, overrides = {}, baseEnv = process.env) {
  return cleanChildEnv(overrides, baseEnv);
}
