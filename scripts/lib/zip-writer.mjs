import fs from 'node:fs';
import path from 'node:path';
import { deflateRawSync } from 'node:zlib';

const UTF8_FLAG = 0x0800;
const DEFLATE_METHOD = 8;

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
  return String(value).split(path.sep).join('/').replace(/^\/+/, '');
}

function walkFiles(root) {
  const result = [];
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    const entries = fs.readdirSync(current, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) stack.push(full);
      else if (entry.isFile()) result.push(full);
    }
  }
  return result.sort((left, right) => normalizeZipPath(path.relative(root, left)).localeCompare(normalizeZipPath(path.relative(root, right))));
}

function localFileHeader(entry) {
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
  const fileType = stat.mode & 0o170000 || 0o100000;
  const fileMode = stat.mode & 0o777 || 0o644;
  const permissions = data.subarray(0, 2).toString('utf8') === '#!' ? fileMode | 0o755 : fileMode;
  return fileType | permissions;
}

function endOfCentralDirectory(entryCount, centralSize, centralOffset) {
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
  const rootName = normalizeZipPath(options.rootName || path.basename(sourceRoot));
  const fixedDate = options.date || new Date(0);
  const { time, date } = dosDateTime(fixedDate);
  const parts = [];
  const entries = [];
  let offset = 0;

  for (const absolutePath of walkFiles(sourceRoot)) {
    const stat = fs.statSync(absolutePath);
    const data = fs.readFileSync(absolutePath);
    const compressed = deflateRawSync(data, { level: 9 });
    const name = Buffer.from(`${rootName}/${normalizeZipPath(path.relative(sourceRoot, absolutePath))}`, 'utf8');
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
  const central = entries.map(centralDirectoryHeader);
  const centralSize = central.reduce((sum, part) => sum + part.length, 0);
  parts.push(...central, endOfCentralDirectory(entries.length, centralSize, centralOffset));

  fs.mkdirSync(path.dirname(archivePath), { recursive: true });
  fs.writeFileSync(archivePath, Buffer.concat(parts));
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
  const entries = [];
  let offset = centralOffset;
  for (let index = 0; index < count; index += 1) {
    if (buffer.readUInt32LE(offset) !== 0x02014b50) throw new Error(`invalid ZIP central directory header at offset ${offset}`);
    const versionMadeBy = buffer.readUInt16LE(offset + 4);
    const nameLength = buffer.readUInt16LE(offset + 28);
    const extraLength = buffer.readUInt16LE(offset + 30);
    const commentLength = buffer.readUInt16LE(offset + 32);
    const externalAttributes = buffer.readUInt32LE(offset + 38);
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
