#!/usr/bin/env node
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import net from "node:net";
import { spawnSync } from "node:child_process";
import { commandNeedsShell, windowsCommandShell } from "./lib/process.mjs";

const args = process.argv.slice(2);
const DEFAULT_TIMEOUT_MS = 90_000;

function usage() {
	return "Usage: node scripts/installer-runtime-smoke.mjs --binary <installed-mcpace-path> [--command <launcher-command>] [--json] [--keep-temp] [--timeout-ms <milliseconds>]";
}

function parseArgs(argv) {
	const parsed = {
		binary: null,
		command: null,
		json: false,
		keepTemp: false,
		timeoutMs: DEFAULT_TIMEOUT_MS,
	};
	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index];
		if (arg === "--binary") {
			parsed.binary = argv[++index] ?? null;
		} else if (arg === "--command") {
			parsed.command = argv[++index] ?? null;
		} else if (arg === "--json") {
			parsed.json = true;
		} else if (arg === "--keep-temp") {
			parsed.keepTemp = true;
		} else if (arg === "--timeout-ms") {
			const value = Number(argv[++index]);
			if (!Number.isInteger(value) || value < 1_000 || value > 300_000) {
				throw new Error(
					"--timeout-ms must be an integer between 1000 and 300000",
				);
			}
			parsed.timeoutMs = value;
		} else if (arg === "--help" || arg === "-h") {
			parsed.help = true;
		} else {
			throw new Error(`unsupported argument: ${arg}`);
		}
	}
	if (!parsed.help && !parsed.binary) throw new Error("--binary is required");
	return parsed;
}

function run(command, commandArgs, timeoutMs) {
	const useWindowsCommandShell = commandNeedsShell(command);
	const result = spawnSync(
		useWindowsCommandShell ? windowsCommandShell() : command,
		useWindowsCommandShell
			? ["/d", "/s", "/c", command, ...commandArgs]
			: commandArgs,
		{
			encoding: "utf8",
			timeout: timeoutMs,
			windowsHide: true,
			maxBuffer: 16 * 1024 * 1024,
		},
	);
	const stdout = result.stdout ?? "";
	const stderr = result.stderr ?? "";
	if (result.error)
		throw new Error(
			`${commandArgs.join(" ")} failed to start: ${result.error.message}`,
		);
	if (result.status !== 0) {
		throw new Error(
			`${commandArgs.join(" ")} exited ${result.status}: ${(stderr || stdout).trim()}`,
		);
	}
	return { stdout, stderr };
}

function parseJson(label, output) {
	try {
		return JSON.parse(output);
	} catch (error) {
		throw new Error(`${label} did not return JSON: ${error.message}`);
	}
}

function requireCondition(condition, message) {
	if (!condition) throw new Error(message);
}

function readBoundedTail(filePath, maxBytes = 8 * 1024) {
	try {
		const stat = fs.lstatSync(filePath);
		if (!stat.isFile() || stat.isSymbolicLink() || stat.size === 0) return null;
		const length = Math.min(stat.size, maxBytes);
		const buffer = Buffer.alloc(length);
		const descriptor = fs.openSync(filePath, "r");
		try {
			const bytesRead = fs.readSync(
				descriptor,
				buffer,
				0,
				length,
				stat.size - length,
			);
			return buffer.subarray(0, bytesRead).toString("utf8").trim() || null;
		} finally {
			fs.closeSync(descriptor);
		}
	} catch {
		return null;
	}
}

function serveLogDiagnostics(root) {
	const serveDir = path.join(root, "data", "runtime", "serve");
	const logs = [
		["serve stderr tail", path.join(serveDir, "stderr.log")],
		["serve stdout tail", path.join(serveDir, "stdout.log")],
	]
		.map(([label, filePath]) => [label, readBoundedTail(filePath)])
		.filter(([, contents]) => contents);
	return logs.length === 0
		? ""
		: `\n${logs.map(([label, contents]) => `${label}:\n${contents}`).join("\n")}`;
}

async function reserveLoopbackPort() {
	const server = net.createServer();
	await new Promise((resolve, reject) => {
		server.once("error", reject);
		server.listen({ host: "127.0.0.1", port: 0 }, resolve);
	});
	const address = server.address();
	await new Promise((resolve, reject) =>
		server.close((error) => (error ? reject(error) : resolve())),
	);
	if (
		!address ||
		typeof address === "string" ||
		!Number.isInteger(address.port)
	) {
		throw new Error("could not reserve a loopback TCP port");
	}
	return address.port;
}

function minimalUpReport(up) {
	return {
		status: up.status,
		endpoint: up.endpoint,
		host: up.host,
		port: up.port,
		checks: Object.fromEntries(
			Object.entries(up.checks ?? {}).filter(([key]) =>
				[
					"initReady",
					"healthOk",
					"mcpInitializeOk",
					"mcpInitializedOk",
					"mcpToolsOk",
					"endpointConfigPersisted",
					"serveRunning",
				].includes(key),
			),
		),
		toolCount: up.mcpTools?.toolCount ?? null,
	};
}

async function smoke(parsed) {
	const binary = path.resolve(parsed.binary);
	const stat = fs.lstatSync(binary);
	requireCondition(
		stat.isFile() && !stat.isSymbolicLink(),
		`installed binary must be a regular non-symlink file: ${binary}`,
	);
	const command = path.resolve(parsed.command ?? binary);
	const commandStat = fs.lstatSync(command);
	requireCondition(
		commandStat.isFile() || commandStat.isSymbolicLink(),
		`installed command must be a file or symlink: ${command}`,
	);

	const tempDir = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-installed-runtime-"),
	);
	const root = path.join(tempDir, "root");
	let version = null;
	let init = null;
	let up = null;
	let stop = null;
	let port = null;
	let cleanupError = null;

	try {
		version = run(command, ["--version"], parsed.timeoutMs).stdout.trim();
		requireCondition(
			/^\d+\.\d+\.\d+(?:[-+][\w.-]+)?$/.test(version),
			`unexpected mcpace version output: ${version}`,
		);

		init = parseJson(
			"init",
			run(command, ["init", "--root", root, "--json"], parsed.timeoutMs).stdout,
		);
		requireCondition(
			init.readyForRuntimeOps === true,
			"init did not report readyForRuntimeOps=true",
		);
		requireCondition(
			init.configVersion === version,
			`init configVersion ${init.configVersion} did not match binary version ${version}`,
		);

		port = await reserveLoopbackPort();
		let upOutput;
		try {
			upOutput = run(
				command,
				[
					"up",
					"--client",
					"none",
					"--no-autostart",
					"--json",
					"--root",
					root,
					"--host",
					"127.0.0.1",
					"--port",
					String(port),
				],
				parsed.timeoutMs,
			).stdout;
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			throw new Error(`${message}${serveLogDiagnostics(root)}`, {
				cause: error,
			});
		}
		up = parseJson("up", upOutput);
		requireCondition(
			up.status === "ready",
			`up status was ${up.status}, expected ready`,
		);
		requireCondition(
			up.endpoint === `http://127.0.0.1:${port}/mcp`,
			`unexpected MCP endpoint: ${up.endpoint}`,
		);
		requireCondition(
			up.host === "127.0.0.1" && up.port === port,
			"up did not retain the requested loopback endpoint",
		);
		for (const check of [
			"initReady",
			"healthOk",
			"mcpInitializeOk",
			"mcpInitializedOk",
			"mcpToolsOk",
			"endpointConfigPersisted",
			"serveRunning",
		]) {
			requireCondition(
				up.checks?.[check] === true,
				`up check ${check} was not true`,
			);
		}
		requireCondition(
			Number.isInteger(up.mcpTools?.toolCount) && up.mcpTools.toolCount > 0,
			"MCP tools/list did not return native tools",
		);

		stop = parseJson(
			"serve stop",
			run(
				command,
				["serve", "stop", "--root", root, "--json"],
				parsed.timeoutMs,
			).stdout,
		);
		requireCondition(
			stop.status === "stopped",
			`serve stop status was ${stop.status}, expected stopped`,
		);

		return {
			schema: "mcpace.installerRuntimeSmoke.v1",
			status: "pass",
			platform: process.platform,
			arch: process.arch,
			binary,
			command,
			version,
			init: {
				configVersion: init.configVersion,
				readyForRuntimeOps: init.readyForRuntimeOps,
			},
			up: minimalUpReport(up),
			stop: { status: stop.status },
			tempDir: parsed.keepTemp ? tempDir : null,
		};
	} finally {
		if (!stop && fs.existsSync(root)) {
			try {
				run(
					command,
					["serve", "stop", "--root", root, "--json"],
					parsed.timeoutMs,
				);
			} catch (error) {
				cleanupError = error;
			}
		}
		if (!parsed.keepTemp) fs.rmSync(tempDir, { recursive: true, force: true });
		if (cleanupError) throw cleanupError;
	}
}

let parsed;
try {
	parsed = parseArgs(args);
	if (parsed.help) {
		process.stdout.write(`${usage()}\n`);
		process.exit(0);
	}
	const report = await smoke(parsed);
	if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
	else
		process.stdout.write(
			`PASS installed runtime smoke: ${report.version} (${report.platform}/${report.arch})\n`,
		);
} catch (error) {
	const report = {
		schema: "mcpace.installerRuntimeSmoke.v1",
		status: "failed",
		error: error?.message ?? String(error),
	};
	if (parsed?.json || args.includes("--json"))
		process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
	else process.stderr.write(`${report.error}\n`);
	process.exitCode = 1;
}
