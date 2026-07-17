#!/usr/bin/env node
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { writeFileAtomicSync } from "./lib/atomic-fs.mjs";
import {
	createVerifiedArtifactCopy,
	releaseBinaryPath,
	sha256File,
	verifyRustProofBinding,
} from "./lib/rust-build-provenance.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");
const argv = process.argv.slice(2);
const jsonOutput = argv.includes("--json");
const writeReport = argv.includes("--write");
const argValue = (name) => {
	const index = argv.indexOf(name);
	return index >= 0 ? argv[index + 1] : null;
};
const outputPath = path.resolve(
	repoRoot,
	argValue("--output") || "reports/live-mcp-e2e-proof.json",
);
function readJsonFile(filePath, label) {
	let source;
	try {
		source = fs.readFileSync(filePath, "utf8");
	} catch (error) {
		throw new Error(`${label} is unavailable: ${error?.message || error}`);
	}
	try {
		return JSON.parse(source);
	} catch (error) {
		throw new Error(`${label} is not valid JSON: ${error?.message || error}`);
	}
}

function resolveBinary() {
	const explicit = argValue("--binary") || process.env.MCPACE_BINARY_PATH;
	const binary = path.resolve(explicit || releaseBinaryPath(repoRoot));
	if (!path.isAbsolute(binary))
		throw new Error("MCPace proof binary must be absolute");
	try {
		if (!fs.statSync(binary).isFile()) throw new Error("path is not a file");
		const selected = fs.realpathSync(binary);
		const expected = fs.realpathSync(releaseBinaryPath(repoRoot));
		const normalize = (value) =>
			process.platform === "win32" ? value.toLowerCase() : value;
		if (normalize(selected) !== normalize(expected)) {
			throw new Error("path is not the canonical target/release artifact");
		}
	} catch (error) {
		throw new Error(
			`MCPace release binary is unavailable at ${binary}: ${error?.message || error}`,
		);
	}
	return binary;
}

function loadRustBuildBinding(binary) {
	const reportPath = path.join(repoRoot, "reports", "rust-live-proof.json");
	const report = readJsonFile(reportPath, "Rust live proof report");
	return verifyRustProofBinding({
		repoRoot,
		binaryPath: binary,
		report,
		proofGeneratorPath: path.join(repoRoot, "scripts", "rust-live-proof.mjs"),
	});
}

function liveProofInputSnapshot() {
	return Object.fromEntries(
		[
			"package.json",
			"reports/rust-live-proof.json",
			"scripts/lib/rust-build-provenance.mjs",
			"scripts/live-mcp-e2e-proof.mjs",
		].map((relativePath) => [
			relativePath,
			sha256File(path.join(repoRoot, relativePath)),
		]),
	);
}

function sameJsonValue(left, right) {
	return JSON.stringify(left) === JSON.stringify(right);
}

async function reservePort() {
	return new Promise((resolve, reject) => {
		const server = net.createServer();
		server.once("error", reject);
		server.listen(0, "127.0.0.1", () => {
			const address = server.address();
			server.close((error) => {
				if (error) reject(error);
				else resolve(address.port);
			});
		});
	});
}

function processIsAlive(pid) {
	if (!Number.isSafeInteger(pid) || pid <= 0) return false;
	try {
		process.kill(pid, 0);
		return true;
	} catch (error) {
		return error?.code !== "ESRCH";
	}
}

async function waitForProcessesToExit(pids, timeoutMs) {
	const deadline = Date.now() + timeoutMs;
	let alive = pids.filter(processIsAlive);
	while (alive.length > 0 && Date.now() < deadline) {
		await delay(50);
		alive = pids.filter(processIsAlive);
	}
	return alive;
}

export async function stopProcessTree(child, ownedPids = []) {
	const leaderPid = Number.isSafeInteger(child?.pid) ? child.pid : null;
	const trackedPids = [leaderPid, ...ownedPids]
		.filter((pid) => Number.isSafeInteger(pid) && pid > 0)
		.filter((pid, index, values) => values.indexOf(pid) === index);
	let taskkillExitCode = null;

	if (process.platform === "win32" && leaderPid) {
		const killer = spawn("taskkill", ["/pid", String(leaderPid), "/t", "/f"], {
			stdio: "ignore",
			windowsHide: true,
		});
		taskkillExitCode = await Promise.race([
			new Promise((resolve) => {
				killer.once("exit", (code) => resolve(code));
				killer.once("error", () => resolve(-1));
			}),
			delay(3_000).then(() => null),
		]);
		if (killer.exitCode === null) killer.kill("SIGKILL");
	} else if (leaderPid) {
		try {
			process.kill(-leaderPid, "SIGTERM");
		} catch {
			if (child.exitCode === null) child.kill("SIGTERM");
		}
		// Give every member of the detached group a bounded graceful-exit window,
		// even when the original group leader has already exited.
		await delay(500);
		try {
			process.kill(-leaderPid, "SIGKILL");
		} catch {
			if (child.exitCode === null) child.kill("SIGKILL");
		}
	}

	let alive = await waitForProcessesToExit(trackedPids, 2_000);
	for (const pid of alive) {
		try {
			process.kill(pid, "SIGKILL");
		} catch {
			// The process may have exited between observation and termination.
		}
	}
	alive = await waitForProcessesToExit(trackedPids, 2_000);
	if (alive.length > 0) {
		throw new Error(
			`process-tree cleanup left owned process ids alive: ${alive.join(", ")}`,
		);
	}
	return {
		leaderPid,
		ownedPids: trackedPids.filter((pid) => pid !== leaderPid),
		containment:
			process.platform === "win32"
				? "kill-on-close-job+verified-pids"
				: "process-group+verified-pids",
		taskkillExitCode,
		verified: true,
	};
}

async function removeTemporaryRoot(root) {
	for (let attempt = 0; attempt < 20; attempt += 1) {
		try {
			fs.rmSync(root, {
				recursive: true,
				force: true,
				maxRetries: 3,
				retryDelay: 50,
			});
			return;
		} catch (error) {
			if (attempt === 19) throw error;
			await delay(100);
		}
	}
}

function fixtureSource() {
	return `import fs from "node:fs";
import readline from "node:readline";
const pidPath = process.argv[2];
if (pidPath) fs.writeFileSync(pidPath, String(process.pid), { mode: 0o600 });
const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
const send = (id, result) => process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id, result }) + "\\n");
input.on("line", (line) => {
  let message;
  try { message = JSON.parse(line); } catch { process.exitCode = 2; return; }
  if (!Object.hasOwn(message, "id")) return;
  if (message.method === "initialize") {
    send(message.id, { protocolVersion: "2025-11-25", capabilities: { tools: {} }, serverInfo: { name: "mcpace-safe-fixture", version: "1.0.0" } });
  } else if (message.method === "tools/list") {
    send(message.id, { tools: [{ name: "echo_read_only", description: "Returns supplied text without side effects", inputSchema: { type: "object", properties: { message: { type: "string" } }, required: ["message"], additionalProperties: false }, annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false } }] });
  } else if (message.method === "tools/call") {
    send(message.id, { content: [{ type: "text", text: "fixture:" + String(message.params?.arguments?.message ?? "") }], isError: false });
  } else {
    send(message.id, {});
  }
});
`;
}

function parseMcpBody(text) {
	if (!text.trim()) return null;
	try {
		return JSON.parse(text);
	} catch {
		const data = text.split(/\r?\n/).find((line) => line.startsWith("data:"));
		if (!data)
			throw new Error(`MCP response is neither JSON nor SSE data: ${text}`);
		return JSON.parse(data.slice(5).trim());
	}
}

async function runProof() {
	const startedAt = Date.now();
	const binary = resolveBinary();
	const bindingBefore = loadRustBuildBinding(binary);
	const { binarySha256, provenance, rustBuildBinding } = bindingBefore;
	const proofInputsBefore = liveProofInputSnapshot();
	const projectVersion = readJsonFile(
		path.join(repoRoot, "package.json"),
		"root package.json",
	).version;
	const port = await reservePort();
	const root = fs.mkdtempSync(path.join(os.tmpdir(), "mcpace-live-e2e-"));
	const fixture = path.join(root, "safe-fixture.mjs");
	const fixturePidPath = path.join(root, "safe-fixture.pid");
	const executionBinary = path.join(
		root,
		process.platform === "win32" ? "mcpace-proof.exe" : "mcpace-proof",
	);
	const baseUrl = `http://127.0.0.1:${port}`;
	let child;
	let executionArtifactBefore;
	let executionArtifactAfterSha256;
	let cleanupEvidence;
	let fixturePid;
	let completedUserPath = false;
	let stdout = "";
	let stderr = "";
	const steps = [];
	const pass = (id, detail) => steps.push({ id, status: "pass", detail });

	try {
		executionArtifactBefore = createVerifiedArtifactCopy(
			binary,
			executionBinary,
			binarySha256,
		);
		fs.writeFileSync(
			path.join(root, "mcpace.config.json"),
			`${JSON.stringify(
				{
					version: projectVersion,
					client: { keyName: "MCPace" },
					profiles: {
						runtime: {
							default: "safe",
							profiles: {
								safe: { description: "Safe", serverOverrides: {} },
							},
						},
					},
					servers: {},
				},
				null,
				2,
			)}\n`,
		);
		fs.writeFileSync(fixture, fixtureSource());
		child = spawn(
			executionBinary,
			[
				"advanced",
				"runtime",
				"foreground",
				"--root",
				root,
				"--host",
				"127.0.0.1",
				"--port",
				String(port),
			],
			{
				cwd: repoRoot,
				detached: process.platform !== "win32",
				env: {
					...process.env,
					MCPACE_KILL_PROCESS_TREE_ON_EXIT: "1",
				},
				windowsHide: true,
				stdio: ["ignore", "pipe", "pipe"],
			},
		);
		child.stdout.on("data", (chunk) => {
			stdout = `${stdout}${chunk}`.slice(-16_384);
		});
		child.stderr.on("data", (chunk) => {
			stderr = `${stderr}${chunk}`.slice(-16_384);
		});

		for (let attempt = 0; attempt < 100; attempt += 1) {
			if (child.exitCode !== null)
				throw new Error(`serve exited ${child.exitCode}: ${stderr || stdout}`);
			try {
				const response = await fetch(`${baseUrl}/healthz`, {
					signal: AbortSignal.timeout(1_000),
				});
				if (response.ok) break;
			} catch {
				// The listener may not have bound yet.
			}
			if (attempt === 99) throw new Error(`serve health timeout: ${stderr}`);
			await delay(100);
		}
		pass("serve-ready", "Unified loopback serve returned a healthy response");

		const action = async (route, body) => {
			const response = await fetch(`${baseUrl}${route}`, {
				method: "POST",
				headers: { "content-type": "application/json" },
				body: JSON.stringify(body),
				signal: AbortSignal.timeout(30_000),
			});
			const text = await response.text();
			if (!response.ok)
				throw new Error(`${route} returned ${response.status}: ${text}`);
			let value;
			try {
				value = JSON.parse(text);
			} catch (error) {
				throw new Error(`${route} returned invalid JSON: ${error.message}`);
			}
			if (value?.result?.ok === false || value?.result?.error)
				throw new Error(`${route} failed: ${text}`);
			return value;
		};

		const commandPath = fixture.replaceAll("\\", "/");
		const pidPath = fixturePidPath.replaceAll("\\", "/");
		await action("/api/actions/server-install-command", {
			commandLine: `node "${commandPath}" "${pidPath}"`,
			server: "safe-fixture",
			disabled: true,
			dryRun: false,
			force: false,
		});
		pass(
			"dashboard-add-disabled",
			"Dashboard saved the harmless fixture disabled",
		);
		await action("/api/actions/server-enable", { server: "safe-fixture" });
		pass("dashboard-enable", "Dashboard intentionally enabled the fixture");
		await action("/api/actions/server-test", {
			server: "safe-fixture",
			timeoutMs: 10_000,
		});
		pass(
			"dashboard-test",
			"Dashboard completed initialize and tools/list Test",
		);

		const mcp = async (body, sessionId = null, method = "POST") => {
			const headers = {
				accept: "application/json, text/event-stream",
				"content-type": "application/json",
			};
			if (sessionId) {
				headers["mcp-session-id"] = sessionId;
				headers["mcp-protocol-version"] = "2025-11-25";
			}
			const response = await fetch(`${baseUrl}/mcp`, {
				method,
				headers,
				body: method === "DELETE" ? undefined : JSON.stringify(body),
				signal: AbortSignal.timeout(30_000),
			});
			const text = await response.text();
			if (!response.ok && response.status !== 202)
				throw new Error(`/mcp returned ${response.status}: ${text}`);
			return { response, value: parseMcpBody(text) };
		};

		const initialized = await mcp({
			jsonrpc: "2.0",
			id: 1,
			method: "initialize",
			params: {
				protocolVersion: "2025-11-25",
				capabilities: {},
				clientInfo: { name: "mcpace-live-proof", version: "1.0.0" },
			},
		});
		const sessionId = initialized.response.headers.get("mcp-session-id");
		if (!sessionId) throw new Error("MCP initialize omitted Mcp-Session-Id");
		await mcp(
			{ jsonrpc: "2.0", method: "notifications/initialized", params: {} },
			sessionId,
		);
		pass(
			"client-initialize",
			"A real Streamable HTTP client initialized a session",
		);

		const listed = await mcp(
			{ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} },
			sessionId,
		);
		const names = listed.value?.result?.tools?.map((tool) => tool.name) || [];
		if (!names.includes("upstream_call"))
			throw new Error("MCP tools/list omitted upstream_call");
		pass("client-tools-list", "Client tools/list exposed upstream_call");

		const called = await mcp(
			{
				jsonrpc: "2.0",
				id: 3,
				method: "tools/call",
				params: {
					name: "upstream_call",
					arguments: {
						server: "safe-fixture",
						tool: "echo_read_only",
						arguments: { message: "read-only-proof" },
						resultMode: "compact",
					},
				},
			},
			sessionId,
		);
		if (!JSON.stringify(called.value).includes("fixture:read-only-proof"))
			throw new Error(
				"upstream_call did not return the read-only fixture result",
			);
		pass(
			"client-read-only-upstream-call",
			"Client received fixture:read-only-proof",
		);

		const resources = await fetch(`${baseUrl}/api/resources`, {
			signal: AbortSignal.timeout(5_000),
		}).then((response) => response.json());
		if (!JSON.stringify(resources).includes("safe-fixture"))
			throw new Error(
				"runtime resources omitted the live safe-fixture session",
			);
		pass(
			"runtime-resource-row",
			"Runtime resources exposed the live fixture session",
		);
		await mcp(null, sessionId, "DELETE");
		completedUserPath = true;
	} finally {
		try {
			if (fs.existsSync(fixturePidPath)) {
				fixturePid = Number(fs.readFileSync(fixturePidPath, "utf8").trim());
			}
			cleanupEvidence = await stopProcessTree(
				child,
				Number.isSafeInteger(fixturePid) ? [fixturePid] : [],
			);
			if (executionArtifactBefore && fs.existsSync(executionBinary)) {
				executionArtifactAfterSha256 = sha256File(executionBinary);
			}
		} finally {
			await removeTemporaryRoot(root);
		}
	}

	if (
		completedUserPath &&
		(!Number.isSafeInteger(fixturePid) || !cleanupEvidence?.verified)
	) {
		throw new Error(
			"process-tree cleanup could not verify termination of the live fixture",
		);
	}
	pass(
		"process-tree-cleanup",
		"The proof leader and owned upstream fixture were verified terminated",
	);

	const bindingAfter = loadRustBuildBinding(binary);
	const proofInputsAfter = liveProofInputSnapshot();
	if (
		binarySha256 !== bindingAfter.binarySha256 ||
		provenance.fingerprint !== bindingAfter.provenance.fingerprint ||
		provenance.fileCount !== bindingAfter.provenance.fileCount ||
		!sameJsonValue(rustBuildBinding, bindingAfter.rustBuildBinding) ||
		!sameJsonValue(proofInputsBefore, proofInputsAfter) ||
		executionArtifactBefore?.sha256 !== binarySha256 ||
		executionArtifactAfterSha256 !== binarySha256
	) {
		throw new Error(
			"release binary, Rust sources, proof generators, or proof reports changed during the live MCP proof",
		);
	}
	return {
		schema: "mcpace.liveMcpE2eProof.v1",
		generatedAt: new Date().toISOString(),
		status: "pass",
		root: ".",
		rootName: path.basename(repoRoot),
		platform: process.platform,
		arch: process.arch,
		binarySha256: bindingAfter.binarySha256,
		binaryStability: {
			selectedBeforeSha256: binarySha256,
			privateCopyBeforeSha256: executionArtifactBefore.sha256,
			privateCopyAfterSha256: executionArtifactAfterSha256,
			selectedAfterSha256: bindingAfter.binarySha256,
			strategy: "private-hash-verified-copy",
			stable: true,
		},
		sourceFingerprint: bindingAfter.provenance.fingerprint,
		sourceFileCount: bindingAfter.provenance.fileCount,
		sourceFiles: bindingAfter.provenance.fileHashes,
		rustBuildBinding: bindingAfter.rustBuildBinding,
		proofGeneratorSha256: proofInputsAfter["scripts/live-mcp-e2e-proof.mjs"],
		proofInputs: proofInputsAfter,
		proofInputSnapshots: {
			before: proofInputsBefore,
			after: proofInputsAfter,
		},
		processTreeCleanup: cleanupEvidence,
		durationMs: Date.now() - startedAt,
		steps,
		readOnlyResult: "fixture:read-only-proof",
	};
}

async function main() {
	let report;
	try {
		report = await runProof();
	} catch (error) {
		report = {
			schema: "mcpace.liveMcpE2eProof.v1",
			generatedAt: new Date().toISOString(),
			status: "fail",
			error: error instanceof Error ? error.message : String(error),
		};
	}

	if (writeReport && report.status === "pass") {
		writeFileAtomicSync(outputPath, `${JSON.stringify(report, null, 2)}\n`, {
			mode: 0o644,
		});
	}
	if (jsonOutput || writeReport)
		process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
	else {
		process.stdout.write(
			`${report.status === "pass" ? "PASS" : "FAIL"} live MCP E2E proof${report.error ? `: ${report.error}` : ""}\n`,
		);
	}
	process.exitCode = report.status === "pass" ? 0 : 1;
}

const isMain =
	process.argv[1] &&
	path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) await main();
