#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
import { repoRoot } from './lib/project-metadata.mjs';

const DEFAULT_VENDOR_BINARY = path.join(repoRoot, 'packages', 'npm', 'cli', 'vendor', 'linux-x64-gnu', process.platform === 'win32' ? 'mcpace.exe' : 'mcpace');

function parseArgs(argv) {
  const parsed = { json: false, write: null, markdown: null, originalRoot: null, previousReadyZip: null, currentReadyZip: null, originalNpmTarball: null, currentNpmTarball: null, installRoot: null, vendorBinary: DEFAULT_VENDOR_BINARY, top: 12 };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = path.resolve(argv[++index] || ''); break;
      case '--markdown': parsed.markdown = path.resolve(argv[++index] || ''); break;
      case '--original-root': parsed.originalRoot = path.resolve(argv[++index] || ''); break;
      case '--previous-ready-zip': parsed.previousReadyZip = path.resolve(argv[++index] || ''); break;
      case '--current-ready-zip': parsed.currentReadyZip = path.resolve(argv[++index] || ''); break;
      case '--original-npm-tarball': parsed.originalNpmTarball = path.resolve(argv[++index] || ''); break;
      case '--current-npm-tarball': parsed.currentNpmTarball = path.resolve(argv[++index] || ''); break;
      case '--install-root': parsed.installRoot = path.resolve(argv[++index] || ''); break;
      case '--vendor-binary': parsed.vendorBinary = path.resolve(argv[++index] || ''); break;
      case '--top': parsed.top = Math.max(1, Number.parseInt(argv[++index] || '12', 10) || 12); break;
      default: throw new Error(`unsupported ready-footprint-audit argument: ${token}`);
    }
  }
  return parsed;
}
function exists(filePath) { return Boolean(filePath) && fs.existsSync(filePath); }
function human(bytes) { if (!Number.isFinite(bytes)) return 'n/a'; const units=['B','KiB','MiB','GiB']; let v=bytes,i=0; while(v>=1024&&i<units.length-1){v/=1024;i++} return `${v.toFixed(i===0?0:2)} ${units[i]}`; }
function sha(filePath) { const r=spawnSync('sha256sum',[filePath],{encoding:'utf8',timeout:30000}); return r.status===0 ? r.stdout.trim().split(/\s+/)[0] : null; }
function statFile(filePath) { if(!exists(filePath)) return null; const st=fs.statSync(filePath); if(!st.isFile()) return null; return { path:filePath, bytes:st.size, human:human(st.size), sha256:sha(filePath) }; }
function walk(dir, cb) { for(const e of fs.readdirSync(dir,{withFileTypes:true})){ const p=path.join(dir,e.name); if(e.isDirectory()) walk(p,cb); else if(e.isFile()) cb(p,fs.statSync(p)); } }
function statDir(dir, top=12) { if(!exists(dir) || !fs.statSync(dir).isDirectory()) return null; let bytes=0,files=0; const topFiles=[]; const topLevel=new Map(); walk(dir,(p,st)=>{ bytes+=st.size; files++; const rel=path.relative(dir,p).split(path.sep).join('/'); topFiles.push({path:rel,bytes:st.size,human:human(st.size)}); const first=rel.split('/')[0]||rel; const cur=topLevel.get(first)||{path:first,bytes:0,files:0}; cur.bytes+=st.size; cur.files++; topLevel.set(first,cur); }); topFiles.sort((a,b)=>b.bytes-a.bytes||a.path.localeCompare(b.path)); const topLevelRows=[...topLevel.values()].map(x=>({...x,human:human(x.bytes)})).sort((a,b)=>b.bytes-a.bytes||a.path.localeCompare(b.path)); return { path:dir, bytes, human:human(bytes), files, topFiles:topFiles.slice(0,top), topLevel:topLevelRows.slice(0,top) }; }
function compare(label,before,after){ if(!before||!after) return null; const delta=after.bytes-before.bytes; return { label, beforeBytes:before.bytes, beforeHuman:before.human, afterBytes:after.bytes, afterHuman:after.human, deltaBytes:delta, deltaHuman:human(Math.abs(delta)), deltaPercent: before.bytes>0 ? delta/before.bytes*100 : null }; }
function collect(options){ const top=options.top||12; const currentProject=statDir(repoRoot,top); const originalRoot=options.originalRoot?statDir(options.originalRoot,top):null; const previousReadyZip=options.previousReadyZip?statFile(options.previousReadyZip):null; const currentReadyZip=options.currentReadyZip?statFile(options.currentReadyZip):null; const originalNpmTarball=options.originalNpmTarball?statFile(options.originalNpmTarball):null; const currentNpmTarball=options.currentNpmTarball?statFile(options.currentNpmTarball):null; const installRoot=options.installRoot?statDir(options.installRoot,top):null; const vendorBinary=options.vendorBinary?statFile(options.vendorBinary):null; return { schema:'mcpace.readyFootprintAudit.v1', generatedAt:new Date().toISOString(), platform:{os:process.platform,arch:process.arch,cpus:os.cpus().length,node:process.version}, currentProject, originalRoot, previousReadyZip, currentReadyZip, originalNpmTarball, currentNpmTarball, installRoot, vendorBinary, comparisons:[compare('source tree: original upload -> current ready project',originalRoot,currentProject),compare('ready ZIP: previous -> current',previousReadyZip,currentReadyZip),compare('npm package: original source-only -> current vendored binary',originalNpmTarball,currentNpmTarball)].filter(Boolean) }; }
function md(report){ const comp=report.comparisons.map(e=>`| ${e.label} | ${e.beforeHuman} | ${e.afterHuman} | ${e.deltaBytes>=0?'+':'-'}${e.deltaHuman} | ${e.deltaPercent==null?'n/a':`${e.deltaPercent>=0?'+':''}${e.deltaPercent.toFixed(2)}%`} |`).join('\n')||'| n/a | n/a | n/a | n/a | n/a |'; const top=(rows)=>rows&&rows.length?rows.map(e=>`| ${e.path} | ${e.human} | ${e.files??''} |`).join('\n'):'| n/a | n/a | n/a |'; const files=(rows)=>rows&&rows.length?rows.map(e=>`| ${e.path} | ${e.human} |`).join('\n'):'| n/a | n/a |'; return `# MCPace ready footprint audit\n\nGenerated: ${report.generatedAt}\n\n## Summary\n\n| Metric | Value |\n|---|---:|\n| Current project files | ${report.currentProject?.files??'n/a'} |\n| Current project size | ${report.currentProject?.human??'n/a'} |\n| Current ready ZIP | ${report.currentReadyZip?.human??'n/a'} |\n| Current npm tarball | ${report.currentNpmTarball?.human??'n/a'} |\n| npm install footprint | ${report.installRoot?.human??'n/a'} |\n| Vendored binary | ${report.vendorBinary?.human??'n/a'} |\n\n## Before / after\n\n| What | Before | After | Delta | Delta % |\n|---|---:|---:|---:|---:|\n${comp}\n\n## Current project top-level weight\n\n| Path | Size | Files |\n|---|---:|---:|\n${top(report.currentProject?.topLevel)}\n\n## npm install footprint top-level weight\n\n| Path | Size | Files |\n|---|---:|---:|\n${top(report.installRoot?.topLevel)}\n\n## Largest files in current project\n\n| File | Size |\n|---|---:|\n${files(report.currentProject?.topFiles)}\n`; }
function main(){ const args=parseArgs(process.argv.slice(2)); const report=collect(args); if(args.write){fs.mkdirSync(path.dirname(args.write),{recursive:true}); fs.writeFileSync(args.write,JSON.stringify(report,null,2)+'\n');} if(args.markdown){fs.mkdirSync(path.dirname(args.markdown),{recursive:true}); fs.writeFileSync(args.markdown,md(report));} if(args.json) process.stdout.write(JSON.stringify(report,null,2)+'\n'); else process.stdout.write(`${args.write||''}\n${args.markdown||''}\n`); }
if (pathToFileURL(path.resolve(process.argv[1]||'')).href === import.meta.url) main();
