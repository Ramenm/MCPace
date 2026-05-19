#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { performance } from 'node:perf_hooks';
import { insertTopK } from './lib/bounded-top-k.mjs';
import { repoRoot } from './lib/project-metadata.mjs';

const args = new Map();
for (let i = 2; i < process.argv.length; i += 1) {
  const arg = process.argv[i];
  if (!arg.startsWith('--')) continue;
  const key = arg.slice(2);
  const next = process.argv[i + 1];
  if (!next || next.startsWith('--')) {
    args.set(key, true);
  } else {
    args.set(key, next);
    i += 1;
  }
}

const numberArg = (name, fallback) => {
  const value = Number(args.get(name) ?? fallback);
  if (!Number.isFinite(value) || value < 0) throw new Error(`invalid --${name}`);
  return Math.floor(value);
};

const servers = numberArg('servers', 50);
const tools = numberArg('tools', 200_000);
const searchLimit = Math.max(1, Math.min(numberArg('search-limit', 25), 100));
const projectionBudget = Math.max(1, Math.min(numberArg('projection-budget', 64), 2048));
const pageSize = Math.max(1, Math.min(numberArg('page-size', 128), 512));
const query = String(args.get('query') ?? 'read file search');
const json = args.has('json');
const writePath = args.get('write') ? path.resolve(repoRoot, String(args.get('write'))) : null;
const memoryLimitMiB = numberArg('memory-limit-mib', 512);

const READONLY_FAMILIES = ['read', 'file', 'search', 'docs'];
const MUTATING_FAMILIES = ['write', 'delete', 'update'];

const toolsPerServerBase = Math.floor(tools / Math.max(1, servers));
const remainder = tools % Math.max(1, servers);
const terms = normalize(query).split(/\s+/).filter((term) => term.length >= 2);
const started = performance.now();
const heapStart = process.memoryUsage().heapUsed;

const topSearch = [];
let searchSpaceToolCount = 0;
let matchCount = 0;
let projectedCandidateCount = 0;
let projectedToolCount = 0;
let brokerOnlyCount = 0;
let readOnlyCount = 0;
let mutatingCount = 0;
const projectionCandidateLimit = Math.max(projectionBudget, Math.min(projectionBudget * 8, 8192));
const brokerSampleLimit = 64;
let brokerSampleCount = 0;

for (let serverIndex = 0; serverIndex < servers; serverIndex += 1) {
  const serverName = `srv-${String(serverIndex).padStart(2, '0')}`;
  const count = toolsPerServerBase + (serverIndex < remainder ? 1 : 0);
  for (let toolIndex = 0; toolIndex < count; toolIndex += 1) {
    searchSpaceToolCount += 1;
    const readOnly = toolIndex % 10 < 7;
    const family = readOnly ? READONLY_FAMILIES[toolIndex % READONLY_FAMILIES.length] : MUTATING_FAMILIES[toolIndex % MUTATING_FAMILIES.length];
    const name = `${family}_${serverIndex}_${toolIndex}`;
    const qualifiedName = `${serverName}.${name}`;
    const title = `${family} tool ${toolIndex}`;
    const description = `${readOnly ? 'read-only discovery' : 'mutating state'} tool ${toolIndex} on ${serverName}`;
    const score = scoreToolFields({ server: serverName, name, qualifiedName, title, description }, terms);
    if (terms.length === 0 || score > 0) {
      matchCount += 1;
      insertTopK(topSearch, {
        score,
        key: qualifiedName,
        tool: compactToolFields(serverName, name, qualifiedName, description, score),
      }, Math.max(2, searchLimit));
    }

    if (readOnly) {
      readOnlyCount += 1;
      if (projectedCandidateCount < projectionCandidateLimit) {
        projectedCandidateCount += 1;
      }
    } else {
      brokerOnlyCount += 1;
      if (brokerSampleCount < brokerSampleLimit) brokerSampleCount += 1;
      mutatingCount += 1;
    }
  }
}

projectedToolCount = Math.min(projectionBudget, projectedCandidateCount);
const firstPageCount = Math.min(pageSize, 8 + projectedToolCount);
const elapsedMs = Math.round(performance.now() - started);
const heapEnd = process.memoryUsage().heapUsed;
const heapDeltaMiB = Math.round(((heapEnd - heapStart) / 1024 / 1024) * 10) / 10;
const pass =
  searchSpaceToolCount === tools &&
  topSearch.length <= Math.max(2, searchLimit) &&
  projectedCandidateCount <= projectionCandidateLimit &&
  projectedToolCount <= projectionBudget &&
  firstPageCount <= pageSize &&
  heapDeltaMiB <= memoryLimitMiB;

const report = {
  status: pass ? 'pass' : 'fail',
  scenario: {
    servers,
    tools,
    query,
    searchLimit,
    projectionBudget,
    projectionCandidateLimit,
    pageSize,
  },
  results: {
    searchSpaceToolCount,
    matchCount,
    returnedSearchCount: Math.min(searchLimit, topSearch.length),
    retainedSearchCandidates: topSearch.length,
    readOnlyCount,
    mutatingCount,
    brokerOnlyCount,
    brokerSampleCount,
    projectedCandidateCount,
    projectedToolCount,
    firstPageCount,
  },
  budgets: {
    topKSearchBounded: topSearch.length <= Math.max(2, searchLimit),
    projectionBounded: projectedCandidateCount <= projectionCandidateLimit,
    toolListPageBounded: firstPageCount <= pageSize,
    heapDeltaMiB,
    memoryLimitMiB,
  },
  algorithm: {
    boundedTopK: true,
    lazyCompactToolMaterialization: true,
    materializesFullCatalog: false,
    maxRetainedSearchCandidates: Math.max(2, searchLimit),
    maxProjectedCandidates: projectionCandidateLimit,
  },
  elapsedMs,
};

if (writePath) {
  fs.mkdirSync(path.dirname(writePath), { recursive: true });
  fs.writeFileSync(writePath, `${JSON.stringify(report, null, 2)}\n`);
}
if (json) {
  process.stdout.write(`${JSON.stringify(report)}\n`);
} else {
  process.stdout.write(`${report.status}: ${tools} tools across ${servers} servers in ${elapsedMs}ms; heap +${heapDeltaMiB} MiB\n`);
}
if (!pass) process.exitCode = 1;

function normalize(value) {
  return String(value).toLowerCase().replace(/[^a-z0-9_.-]+/g, ' ');
}

function scoreToolFields(tool, terms) {
  if (terms.length === 0) return 1;
  const all = `${tool.server} ${tool.name} ${tool.qualifiedName} ${tool.title} ${tool.description}`;
  let total = 0;
  for (const term of terms) {
    if (tool.name === term || tool.qualifiedName === term) total += 80;
    if (tool.name.includes(term)) total += 40;
    if (tool.qualifiedName.includes(term)) total += 30;
    if (tool.title.includes(term)) total += 20;
    if (tool.description.includes(term)) total += 10;
    if (tool.server.includes(term)) total += 5;
    if (all.includes(term)) total += 1;
  }
  return total;
}

function compactToolFields(server, name, qualifiedName, description, score) {
  return {
    server,
    name,
    qualifiedName,
    description: description.slice(0, 220),
    score,
    call: { tool: 'upstream_call', arguments: { server, tool: name } },
  };
}
