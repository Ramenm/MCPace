import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';

let tempCounter = 0;

const { constants } = fs;
const O_NOFOLLOW = typeof constants.O_NOFOLLOW === 'number' ? constants.O_NOFOLLOW : 0;
const FILE_TYPE_MASK = 0o170000n;
const REGULAR_FILE_TYPE = 0o100000n;
const DEFAULT_LOCK_STALE_MS = 15 * 60 * 1000;

function absolutePath(filePath) {
  return path.resolve(filePath);
}

function modeAsBigInt(stat) {
  return typeof stat.mode === 'bigint' ? stat.mode : BigInt(stat.mode);
}

function isRegularFile(stat) {
  return (modeAsBigInt(stat) & FILE_TYPE_MASK) === REGULAR_FILE_TYPE;
}

function sameOpenedFileSnapshot(left, right) {
  return left.dev === right.dev
    && left.ino === right.ino
    && left.mode === right.mode
    && left.size === right.size
    && left.mtimeNs === right.mtimeNs
    && left.ctimeNs === right.ctimeNs;
}

function sameIdentity(left, right) {
  return left.dev === right.dev && left.ino === right.ino && left.mode === right.mode;
}

function openRegularFileReadOnlyNoFollowSync(filePath) {
  const target = absolutePath(filePath);
  let flags = constants.O_RDONLY;
  if (process.platform !== 'win32' && O_NOFOLLOW) flags |= O_NOFOLLOW;

  if (process.platform === 'win32' || !O_NOFOLLOW) {
    const linkStat = fs.lstatSync(target, { bigint: true });
    if (!isRegularFile(linkStat)) {
      throw new Error(`refusing to follow a non-regular file entry: ${target}`);
    }
  }

  const fd = fs.openSync(target, flags);
  try {
    const stat = fs.fstatSync(fd, { bigint: true });
    if (!isRegularFile(stat)) {
      throw new Error(`opened entry is not a regular file: ${target}`);
    }
    return { fd, stat };
  } catch (error) {
    fs.closeSync(fd);
    throw error;
  }
}

export function readRegularFileStableSync(filePath, options = {}) {
  const target = absolutePath(filePath);
  const maxBytes = options.maxBytes;
  const { fd, stat: before } = openRegularFileReadOnlyNoFollowSync(target);
  try {
    if (maxBytes !== undefined && before.size > BigInt(maxBytes)) {
      throw new Error(`file exceeds maximum allowed size of ${maxBytes} bytes: ${target}`);
    }
    const data = fs.readFileSync(fd);
    const after = fs.fstatSync(fd, { bigint: true });
    if (maxBytes !== undefined && after.size > BigInt(maxBytes)) {
      throw new Error(`file exceeds maximum allowed size of ${maxBytes} bytes: ${target}`);
    }
    if (!sameOpenedFileSnapshot(before, after)) {
      throw new Error(`file changed while it was being read: ${target}`);
    }
    if (BigInt(data.length) !== after.size) {
      throw new Error(`file size changed while it was being read: ${target}`);
    }
    return { data, stat: after };
  } finally {
    fs.closeSync(fd);
  }
}

export function lstatStableDirectorySync(directoryPath) {
  const target = absolutePath(directoryPath);
  const before = fs.lstatSync(target, { bigint: true });
  if (!before.isDirectory()) {
    throw new Error(`expected a directory: ${target}`);
  }
  const entries = fs.readdirSync(target, { withFileTypes: true })
    .sort((left, right) => left.name.localeCompare(right.name));
  const after = fs.lstatSync(target, { bigint: true });
  if (!sameIdentity(before, after)) {
    throw new Error(`directory changed while it was being listed: ${target}`);
  }
  return { entries, stat: after };
}

function tempPathFor(targetPath) {
  const target = absolutePath(targetPath);
  tempCounter += 1;
  const suffix = `${process.pid}-${Date.now()}-${tempCounter}-${crypto.randomBytes(8).toString('hex')}`;
  return path.join(path.dirname(target), `.${path.basename(target)}.tmp-${suffix}`);
}

function fsyncParentDirectoryBestEffort(filePath) {
  if (process.platform === 'win32') return;
  let fd;
  try {
    fd = fs.openSync(path.dirname(filePath), constants.O_RDONLY | (constants.O_DIRECTORY || 0));
    fs.fsyncSync(fd);
  } catch {
    // Directory fsync support varies; the temp file itself was already fsync'd.
  } finally {
    if (fd !== undefined) {
      try { fs.closeSync(fd); } catch {}
    }
  }
}

function normalizeWriteOptions(options = {}) {
  if (typeof options === 'string') return { encoding: options };
  return { ...options };
}

export function writeFileAtomicSync(targetPath, contents, options = {}) {
  const target = absolutePath(targetPath);
  const normalized = normalizeWriteOptions(options);
  const mode = normalized.mode ?? 0o644;
  const data = Buffer.isBuffer(contents) ? contents : Buffer.from(String(contents), normalized.encoding || 'utf8');
  fs.mkdirSync(path.dirname(target), { recursive: true });
  const tempPath = tempPathFor(target);
  let fd;
  let committed = false;
  try {
    fd = fs.openSync(tempPath, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL, mode);
    let offset = 0;
    while (offset < data.length) {
      offset += fs.writeSync(fd, data, offset, data.length - offset);
    }
    fs.fsyncSync(fd);
    fs.closeSync(fd);
    fd = undefined;
    fs.renameSync(tempPath, target);
    committed = true;
    fsyncParentDirectoryBestEffort(target);
  } finally {
    if (fd !== undefined) {
      try { fs.closeSync(fd); } catch {}
    }
    if (!committed) {
      try { fs.rmSync(tempPath, { force: true }); } catch {}
    }
  }
}

export function copyRegularFileNoFollowSync(sourcePath, targetPath, options = {}) {
  const { data, stat } = readRegularFileStableSync(sourcePath, { maxBytes: options.maxBytes });
  const mode = Number(modeAsBigInt(stat) & 0o777n) || 0o644;
  writeFileAtomicSync(targetPath, data, { mode });
  return { mode, size: Number(stat.size) };
}

export function readTextIfFileSync(filePath) {
  try {
    return fs.readFileSync(filePath, 'utf8');
  } catch (error) {
    if (error?.code === 'ENOENT' || error?.code === 'ENOTDIR') return '';
    throw error;
  }
}

export function pathIsFileSync(filePath) {
  try {
    return fs.statSync(filePath).isFile();
  } catch {
    return false;
  }
}

function readLockPayload(lockPath) {
  try {
    return JSON.parse(fs.readFileSync(lockPath, 'utf8'));
  } catch {
    return null;
  }
}


function processExists(pid) {
  const parsed = Number(pid);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) return null;
  try {
    process.kill(parsed, 0);
    return true;
  } catch (error) {
    if (error?.code === 'ESRCH') return false;
    if (error?.code === 'EPERM') return true;
    return null;
  }
}

function recoverableLockReason(payload, staleMs) {
  const createdAtMs = Number(payload?.createdAtMs || 0);
  const ageMs = createdAtMs ? Date.now() - createdAtMs : null;
  const sameHost = payload?.host && payload.host === os.hostname();
  if (!sameHost) return null;

  const pidState = processExists(payload?.pid);
  if (pidState === false) {
    return `owner pid ${payload.pid} is not running on ${payload.host}`;
  }
  if (ageMs !== null && ageMs > staleMs && pidState !== true) {
    return `same-host lock exceeded stale threshold after ${Math.round(ageMs / 1000)}s`;
  }
  return null;
}

function tryRemoveRecoverableLockSync(lockPath, staleMs) {
  let before;
  try {
    before = fs.lstatSync(lockPath, { bigint: true });
  } catch {
    return null;
  }
  if (!isRegularFile(before)) return null;

  const payload = readLockPayload(lockPath);
  const reason = recoverableLockReason(payload, staleMs);
  if (!reason) return null;

  const after = fs.lstatSync(lockPath, { bigint: true });
  if (!sameIdentity(before, after)) return null;
  fs.rmSync(lockPath, { force: true });
  return reason;
}

function lockHeldMessage(lockPath, staleMs) {
  const payload = readLockPayload(lockPath);
  const createdAtMs = Number(payload?.createdAtMs || 0);
  const ageMs = createdAtMs ? Date.now() - createdAtMs : null;
  const staleNote = ageMs !== null && ageMs > staleMs
    ? `; the lock appears stale after ${Math.round(ageMs / 1000)}s, remove it manually after verifying no build is active`
    : '';
  return `lock is already held at ${lockPath}${payload?.pid ? ` by pid ${payload.pid}` : ''}${staleNote}`;
}

export function withFileLockSync(lockPath, callback, options = {}) {
  const target = absolutePath(lockPath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  const staleMs = options.staleMs ?? DEFAULT_LOCK_STALE_MS;
  let fd;
  try {
    for (let attempt = 0; attempt < 2; attempt += 1) {
      try {
        fd = fs.openSync(target, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL, 0o600);
        break;
      } catch (error) {
        if (error?.code === 'EEXIST' && attempt === 0) {
          const recovered = tryRemoveRecoverableLockSync(target, staleMs);
          if (recovered) continue;
        }
        if (error?.code === 'EEXIST') {
          throw new Error(lockHeldMessage(target, staleMs));
        }
        throw error;
      }
    }
    if (fd === undefined) {
      throw new Error(lockHeldMessage(target, staleMs));
    }
    const payload = `${JSON.stringify({ pid: process.pid, host: os.hostname(), createdAtMs: Date.now() })}\n`;
    fs.writeFileSync(fd, payload);
    fs.fsyncSync(fd);
    return callback();
  } finally {
    if (fd !== undefined) {
      try { fs.closeSync(fd); } catch {}
      try { fs.rmSync(target, { force: true }); } catch {}
    }
  }
}
