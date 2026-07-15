import { spawnSync } from 'node:child_process';

export function runPython(args, options = {}) {
  const candidates = process.platform === 'win32'
    ? [['python'], ['py', '-3'], ['python3']]
    : [['python3'], ['python'], ['py', '-3']];

  for (const [command, ...prefixArgs] of candidates) {
    const probe = spawnSync(command, [...prefixArgs, '--version'], {
      encoding: 'utf8',
      windowsHide: true,
      ...options,
    });
    if (probe.error || probe.status !== 0) continue;
    return spawnSync(command, [...prefixArgs, ...args], options);
  }
  throw new Error('no usable Python interpreter found');
}
