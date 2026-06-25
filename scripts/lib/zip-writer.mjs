import fs from 'node:fs';
import path from 'node:path';
import { deflateRawSync } from 'node:zlib';
import { readRegularFileStableSync, writeFileAtomicSync } from './atomic-fs.mjs';

const UTF8_FLAG = 0x0800;
const DEFLATE_METHOD = 8;
const MAX_UINT16 = 0xffff;
const MAX_UINT32 = 0xffffffff;

function assertUInt16(value, label) {
  if (!Number.isSafeInteger(value) || value < 0 || value > MAX_UINT16) {
    throw new Error(`${label} exceeds the classic ZIP uint16 limit; ZIP64 is not implemented`);
  }
}

function assertUInt32(value, label) {
  if (!Number.isSafeInteger(value) || value < 0 || value > MAX_UINT32) {
    throw new Error(`${label} exceeds the classic ZIP uint32 limit; ZIP64 is not implemented`);
  }
}

const crcTable = new Uint32Array(256);
for (let n = 0; n < 256; n += 1) {
  let c = n;
  for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  crcTable[n] = c >>> 0;
}

export function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) crc = crcTable[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function dosDateTime(date = new Date()) {
  const year = Math.max(1980, Math.min(2107, date.getFullYear()));
  const month = Math.max(1, Math.min(12, date.getMonth() + 1));
  const day = Math.max(1, Math.min(31, date.getDate()));
  const hour = Math.max(0, Math.min(23, date.getHours()));
  const minute = Math.max(0, Math.min(59, date.getMinutes()));
  const second = Math.max(0, Math.min(59, Math.floor(date.getSeconds() / 2)));
  return {
    time: (hour << 11) | (minute << 5) | second,
    date: ((year - 1980) << 9) | (month << 5) | day,
  };
}

function normalizeZipPath(value) {
  return String(value).split(/[\\/]+/).join('/').replace(/^\/+/, '');
}

function validateSafeZipPath(value, label = 'ZIP path') {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} must not be empty`);
  }
  if (/^[\\/]/.test(value) || path.isAbsolute(value) || /^[A-Za-z]:[\\/]/.test(value) || /^\\\\/.test(value)) {
    throw new Error(`${label} must be relative`);
  }
  if (/[\u0000-\u001f]/.test(value)) {
    throw new Error(`${label} must not contain control characters`);
  }
  const parts = value.split(/[\\/]+/);
  if (parts.some((part) => part.length === 0 || part === '.' || part === '..')) {
    throw new Error(`${label} must not contain empty, . or .. path segments`);
  }
}

function walkFiles(root) {
  const result = [];
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    const entries = fs.readdirSync(current, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const full = path.join(current, entry.name);
      const stat = fs.lstatSync(full);
      const relative = normalizeZipPath(path.relative(root, full));
      if (stat.isSymbolicLink()) {
        throw new Error(`refusing to archive symbolic link entry: ${relative}`);
      }
      if (stat.isDirectory()) {
        stack.push(full);
      } else if (stat.isFile()) {
        result.push(full);
      } else {
        throw new Error(`refusing to archive non-regular entry: ${relative}`);
      }
    }
  }
  return result.sort((left, right) => normalizeZipPath(path.relative(root, left)).localeCompare(normalizeZipPath(path.relative(root, right))));
}

function assertClassicZipEntry(entry) {
  assertUInt16(entry.name.length, `ZIP entry name length for ${entry.path}`);
  assertUInt32(entry.crc, `ZIP entry CRC for ${entry.path}`);
  assertUInt32(entry.compressedSize, `ZIP compressed size for ${entry.path}`);
  assertUInt32(entry.size, `ZIP uncompressed size for ${entry.path}`);
  assertUInt32(entry.offset, `ZIP local header offset for ${entry.path}`);
}

function localFileHeader(entry) {
  assertClassicZipEntry(entry);
  const header = Buffer.alloc(30);
  header.writeUInt32LE(0x04034b50, 0);
  header.writeUInt16LE(20, 4);
  header.writeUInt16LE(UTF8_FLAG, 6);
  header.writeUInt16LE(DEFLATE_METHOD, 8);
  header.writeUInt16LE(entry.time, 10);
  header.writeUInt16LE(entry.date, 12);
  header.writeUInt32LE(entry.crc, 14);
  header.writeUInt32LE(entry.compressedSize, 18);
  header.writeUInt32LE(entry.size, 22);
  header.writeUInt16LE(entry.name.length, 26);
  header.writeUInt16LE(0, 28);
  return Buffer.concat([header, entry.name]);
}

function centralDirectoryHeader(entry) {
  assertClassicZipEntry(entry);
  const header = Buffer.alloc(46);
  header.writeUInt32LE(0x02014b50, 0);
  header.writeUInt16LE((3 << 8) | 20, 4);
  header.writeUInt16LE(20, 6);
  header.writeUInt16LE(UTF8_FLAG, 8);
  header.writeUInt16LE(DEFLATE_METHOD, 10);
  header.writeUInt16LE(entry.time, 12);
  header.writeUInt16LE(entry.date, 14);
  header.writeUInt32LE(entry.crc, 16);
  header.writeUInt32LE(entry.compressedSize, 20);
  header.writeUInt32LE(entry.size, 24);
  header.writeUInt16LE(entry.name.length, 28);
  header.writeUInt16LE(0, 30);
  header.writeUInt16LE(0, 32);
  header.writeUInt16LE(0, 34);
  header.writeUInt16LE(0, 36);
  header.writeUInt32LE((entry.mode & 0xffff) * 0x10000, 38);
  header.writeUInt32LE(entry.offset, 42);
  return Buffer.concat([header, entry.name]);
}

function unixModeForZip(stat, data) {
  const rawMode = typeof stat.mode === 'bigint' ? stat.mode : BigInt(stat.mode);
  const fileType = Number(rawMode & 0o170000n) || 0o100000;
  const fileMode = Number(rawMode & 0o777n) || 0o644;
  const permissions = data.subarray(0, 2).toString('utf8') === '#!' ? fileMode | 0o755 : fileMode;
  return fileType | permissions;
}

function endOfCentralDirectory(entryCount, centralSize, centralOffset) {
  assertUInt16(entryCount, 'ZIP entry count');
  assertUInt32(centralSize, 'ZIP central directory size');
  assertUInt32(centralOffset, 'ZIP central directory offset');
  const header = Buffer.alloc(22);
  header.writeUInt32LE(0x06054b50, 0);
  header.writeUInt16LE(0, 4);
  header.writeUInt16LE(0, 6);
  header.writeUInt16LE(entryCount, 8);
  header.writeUInt16LE(entryCount, 10);
  header.writeUInt32LE(centralSize, 12);
  header.writeUInt32LE(centralOffset, 16);
  header.writeUInt16LE(0, 20);
  return header;
}

export function createZipFromDirectory(sourceRoot, archivePath, options = {}) {
  const rawRootName = String(options.rootName || path.basename(sourceRoot));
  const maxFileBytes = options.maxFileBytes;
  const maxTotalUncompressedBytes = options.maxTotalUncompressedBytes;
  validateSafeZipPath(rawRootName, 'ZIP root name');
  const rootName = normalizeZipPath(rawRootName);
  const fixedDate = options.date || new Date(0);
  const { time, date } = dosDateTime(fixedDate);
  const parts = [];
  const entries = [];
  const seenPaths = new Set();
  let offset = 0;
  let totalUncompressedBytes = 0;

  for (const absolutePath of walkFiles(sourceRoot)) {
    const rawRelative = path.relative(sourceRoot, absolutePath);
    validateSafeZipPath(rawRelative, 'ZIP entry path');
    const relative = normalizeZipPath(rawRelative);
    const { data, stat } = readRegularFileStableSync(absolutePath, { maxBytes: maxFileBytes });
    const compressed = deflateRawSync(data, { level: 9 });
    assertUInt32(data.length, `ZIP uncompressed size for ${relative}`);
    totalUncompressedBytes += data.length;
    if (maxTotalUncompressedBytes !== undefined && totalUncompressedBytes > maxTotalUncompressedBytes) {
      throw new Error(`ZIP input exceeds maximum total uncompressed size of ${maxTotalUncompressedBytes} bytes`);
    }
    assertUInt32(compressed.length, `ZIP compressed size for ${relative}`);
    assertUInt32(offset, `ZIP local header offset for ${relative}`);
    const zipPath = `${rootName}/${relative}`;
    if (seenPaths.has(zipPath)) {
      throw new Error(`duplicate ZIP entry path after normalization: ${zipPath}`);
    }
    seenPaths.add(zipPath);
    const name = Buffer.from(zipPath, 'utf8');
    assertUInt16(name.length, `ZIP entry name length for ${zipPath}`);
    const entry = {
      name,
      path: name.toString('utf8'),
      crc: crc32(data),
      size: data.length,
      compressedSize: compressed.length,
      time,
      date,
      mode: unixModeForZip(stat, data),
      offset,
    };
    const header = localFileHeader(entry);
    parts.push(header, compressed);
    offset += header.length + compressed.length;
    entries.push(entry);
  }

  const centralOffset = offset;
  assertUInt32(centralOffset, 'ZIP central directory offset');
  assertUInt16(entries.length, 'ZIP entry count');
  const central = entries.map(centralDirectoryHeader);
  const centralSize = central.reduce((sum, part) => sum + part.length, 0);
  assertUInt32(centralSize, 'ZIP central directory size');
  parts.push(...central, endOfCentralDirectory(entries.length, centralSize, centralOffset));

  writeFileAtomicSync(archivePath, Buffer.concat(parts), { mode: 0o644 });
  return entries.map((entry) => entry.path);
}

function findEndOfCentralDirectory(buffer) {
  const minimumSize = 22;
  const searchStart = Math.max(0, buffer.length - 0xffff - minimumSize);
  for (let offset = buffer.length - minimumSize; offset >= searchStart; offset -= 1) {
    if (buffer.readUInt32LE(offset) === 0x06054b50) return offset;
  }
  throw new Error('ZIP end of central directory was not found');
}

export function listZipEntryMetadata(archivePath) {
  const buffer = fs.readFileSync(archivePath);
  const eocd = findEndOfCentralDirectory(buffer);
  const count = buffer.readUInt16LE(eocd + 10);
  const centralOffset = buffer.readUInt32LE(eocd + 16);
  if (centralOffset > buffer.length) {
    throw new Error('ZIP central directory offset points outside the archive');
  }
  const entries = [];
  let offset = centralOffset;
  for (let index = 0; index < count; index += 1) {
    if (offset + 46 > buffer.length) throw new Error(`truncated ZIP central directory header at offset ${offset}`);
    if (buffer.readUInt32LE(offset) !== 0x02014b50) throw new Error(`invalid ZIP central directory header at offset ${offset}`);
    const versionMadeBy = buffer.readUInt16LE(offset + 4);
    const nameLength = buffer.readUInt16LE(offset + 28);
    const extraLength = buffer.readUInt16LE(offset + 30);
    const commentLength = buffer.readUInt16LE(offset + 32);
    const externalAttributes = buffer.readUInt32LE(offset + 38);
    if (offset + 46 + nameLength + extraLength + commentLength > buffer.length) {
      throw new Error(`truncated ZIP central directory entry at offset ${offset}`);
    }
    entries.push({
      name: buffer.subarray(offset + 46, offset + 46 + nameLength).toString('utf8'),
      hostSystem: versionMadeBy >>> 8,
      externalAttributes,
      unixMode: (externalAttributes >>> 16) & 0xffff,
    });
    offset += 46 + nameLength + extraLength + commentLength;
  }
  return entries;
}

export function listZipEntries(archivePath) {
  return listZipEntryMetadata(archivePath).map((entry) => entry.name);
}
