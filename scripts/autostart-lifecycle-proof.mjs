#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";

const argv = process.argv.slice(2);

function usage() {
	return [
		"Usage: node scripts/autostart-lifecycle-proof.mjs --binary <path> [options]",
		"",
		"Options:",
		"  --binary <path>          Native mcpace binary (required)",
		"  --timeout-ms <n>         Per-command timeout (default: 60000)",
		"  --recovery-timeout-ms <n> Crash-recovery deadline (default: 45000)",
		"  --allow-manager-skip     Controlled skip only for a missing user manager/session domain",
		"  --confirm-disposable-user Confirm this is an isolated disposable OS user/runner",
		"  --keep-temp              Preserve the isolated root for diagnostics",
		"  --json                   Emit JSON only",
	].join("\n");
}

function parseArgs(values) {
	const parsed = {
		binary: null,
		timeoutMs: 60_000,
		recoveryTimeoutMs: 45_000,
		allowManagerSkip: false,
		confirmDisposableUser: false,
		keepTemp: false,
		json: false,
		help: false,
	};
	for (let index = 0; index < values.length; index += 1) {
		const value = values[index];
		if (value === "--help" || value === "-h") parsed.help = true;
		else if (value === "--allow-manager-skip") parsed.allowManagerSkip = true;
		else if (value === "--confirm-disposable-user")
			parsed.confirmDisposableUser = true;
		else if (value === "--keep-temp") parsed.keepTemp = true;
		else if (value === "--json") parsed.json = true;
		else if (value === "--binary") parsed.binary = values[++index];
		else if (value === "--timeout-ms") parsed.timeoutMs = Number(values[++index]);
		else if (value === "--recovery-timeout-ms")
			parsed.recoveryTimeoutMs = Number(values[++index]);
		else throw new Error(`unknown argument: ${value}`);
	}
	if (!parsed.help && !parsed.binary) throw new Error("--binary is required");
	for (const [name, value] of [
		["timeout-ms", parsed.timeoutMs],
		["recovery-timeout-ms", parsed.recoveryTimeoutMs],
	]) {
		if (!Number.isSafeInteger(value) || value <= 0)
			throw new Error(`--${name} must be a positive integer`);
	}
	return parsed;
}

function parseJson(text, label) {
	try {
		return JSON.parse(text);
	} catch (error) {
		throw new Error(`${label} returned invalid JSON: ${error.message}\n${text}`, {
			cause: error,
		});
	}
}

function run(binary, args, timeoutMs, allowFailure = false) {
	const result = spawnSync(binary, args, {
		encoding: "utf8",
		timeout: timeoutMs,
		windowsHide: true,
		env: { ...process.env, MCPACE_KILL_PROCESS_TREE_ON_EXIT: "1" },
	});
	const output = {
		command: [binary, ...args].join(" "),
		status: result.status,
		stdout: result.stdout ?? "",
		stderr: result.stderr ?? "",
		error: result.error?.message ?? null,
	};
	if (!allowFailure && (result.error || result.status !== 0)) {
		throw new Error(
			`${output.command} failed (${result.status ?? "spawn"}): ${output.stderr || output.stdout || output.error}`,
			{ cause: result.error },
		);
	}
	return output;
}

function runJson(binary, args, timeoutMs, allowFailure = false) {
	const result = run(binary, args, timeoutMs, allowFailure);
	const trimmed = result.stdout.trim();
	return {
		...result,
		json: trimmed ? parseJson(trimmed, args.join(" ")) : null,
	};
}

function managerUnavailable(detail) {
	const normalized = detail.toLowerCase();
	return [
		"failed to connect to bus: no medium found",
		"failed to connect to bus: no such file or directory",
		"failed to connect to bus: host is down",
		"system has not been booted with systemd as init system",
		"could not find domain for",
		"no such process",
		"launchctl",
	].some((needle) => normalized.includes(needle));
}

async function reserveLoopbackPort() {
	return await new Promise((resolve, reject) => {
		const server = net.createServer();
		server.unref();
		server.once("error", reject);
		server.listen(0, "127.0.0.1", () => {
			const address = server.address();
			const port = typeof address === "object" && address ? address.port : null;
			server.close((error) => {
				if (error) reject(error);
				else if (!port) reject(new Error("failed to reserve loopback port"));
				else resolve(port);
			});
		});
	});
}

function runtimePid(statusReport) {
	const value = statusReport?.runtime?.report?.pid;
	return Number.isSafeInteger(value) && value > 0 ? value : null;
}

function killRuntime(pid) {
	if (!Number.isSafeInteger(pid) || pid <= 0 || pid === process.pid)
		throw new Error(`refusing unsafe runtime PID: ${pid}`);
	if (process.platform === "win32") {
		const result = spawnSync("taskkill.exe", ["/PID", String(pid), "/T", "/F"], {
			encoding: "utf8",
			windowsHide: true,
		});
		if (result.status !== 0)
			throw new Error(`taskkill failed for ${pid}: ${result.stderr || result.stdout}`);
		return;
	}
	process.kill(pid, "SIGKILL");
}

function delay(ms) {
	return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForRecovery(binary, root, previousPid, timeoutMs, commandTimeoutMs) {
	const deadline = Date.now() + timeoutMs;
	let last = null;
	while (Date.now() < deadline) {
		await delay(500);
		const result = runJson(
			binary,
			["status", "--root", root, "--json"],
			commandTimeoutMs,
			true,
		);
		last = result;
		const pid = runtimePid(result.json);
		if (result.status === 0 && result.json?.ok === true && pid && pid !== previousPid)
			return result.json;
	}
	throw new Error(
		`runtime did not recover from PID ${previousPid} within ${timeoutMs}ms: ${last?.stderr || last?.stdout || "no status"}`,
	);
}

async function proof(parsed) {
	if (
		!parsed.confirmDisposableUser ||
		process.env.MCPACE_DISPOSABLE_AUTOSTART_PROOF !== "1"
	) {
		throw new Error(
			"refusing to modify the current user's login startup: run only in a disposable user/runner with --confirm-disposable-user and MCPACE_DISPOSABLE_AUTOSTART_PROOF=1",
		);
	}
	const binary = path.resolve(parsed.binary);
	if (!fs.statSync(binary).isFile()) throw new Error(`binary is not a file: ${binary}`);
	const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "mcpace-autostart-proof-"));
	const root = path.join(tempDir, "root");
	const port = await reserveLoopbackPort();
	let installed = false;
	let cleanup = null;
	const evidence = {};

	try {
		const upArgs = [
			"up",
			"--client",
			"none",
			"--root",
			root,
			"--host",
			"127.0.0.1",
			"--port",
			String(port),
			"--json",
		];
		const up = runJson(binary, upArgs, parsed.timeoutMs, true);
		if (up.status !== 0) {
			const detail = `${up.stderr}\n${up.stdout}`;
			if (parsed.allowManagerSkip && managerUnavailable(detail)) {
				return {
					schema: "mcpace.autostartLifecycleProof.v1",
					status: "skipped",
					reason: "user login manager/session domain unavailable on this disposable runner",
					platform: process.platform,
					arch: process.arch,
					detail: detail.trim(),
				};
			}
			throw new Error(`mcpace up failed: ${detail.trim()}`);
		}
		installed = true;
		evidence.up = up.json;

		const repair = runJson(binary, upArgs, parsed.timeoutMs);
		if (repair.json?.status !== "ready") {
			throw new Error("second convergent up did not report status=ready");
		}
		evidence.repair = repair.json;

		const verify = runJson(
			binary,
			["advanced", "autostart", "verify", "--root", root, "--json"],
			parsed.timeoutMs,
		);
		if (verify.json?.ok !== true) throw new Error("autostart verification reported ok=false");
		evidence.verify = verify.json;

		const activation = runJson(
			binary,
			["advanced", "autostart", "prove", "--root", root, "--json"],
			parsed.timeoutMs,
		);
		if (
			activation.json?.ok !== true ||
			activation.json?.proof?.activationAttempted !== true ||
			activation.json?.proof?.endpointVerified !== true ||
			activation.json?.proof?.supervisorVerified !== true ||
			activation.json?.proof?.restoredInitialState !== true
		) {
			throw new Error("autostart activation proof is incomplete");
		}
		evidence.activation = activation.json;

		const beforeCrash = runJson(
			binary,
			["status", "--root", root, "--json"],
			parsed.timeoutMs,
		);
		const pid = runtimePid(beforeCrash.json);
		if (!pid || beforeCrash.json?.ok !== true)
			throw new Error("status did not return a verified live runtime PID");
		evidence.beforeCrash = beforeCrash.json;

		killRuntime(pid);
		evidence.afterCrash = await waitForRecovery(
			binary,
			root,
			pid,
			parsed.recoveryTimeoutMs,
			parsed.timeoutMs,
		);
		const recoveryOwnership = runJson(
			binary,
			[
				"advanced",
				"autostart",
				"prove",
				"--dry-run",
				"--root",
				root,
				"--json",
			],
			parsed.timeoutMs,
		);
		if (
			recoveryOwnership.json?.proof?.dryRun !== true ||
			recoveryOwnership.json?.proof?.initialRuntimeActive !== true ||
			recoveryOwnership.json?.proof?.endpointVerified !== true ||
			recoveryOwnership.json?.proof?.supervisorVerified !== true
		) {
			throw new Error(
				"recovered runtime is healthy but is not owned by the registered user supervisor",
			);
		}
		evidence.recoveryOwnership = recoveryOwnership.json;

		return {
			schema: "mcpace.autostartLifecycleProof.v1",
			status: "pass",
			platform: process.platform,
			arch: process.arch,
			root: parsed.keepTemp ? root : null,
			port,
			evidence,
		};
	} finally {
		let cleanupError = null;
		if (installed || fs.existsSync(root)) {
			cleanup = runJson(
				binary,
				[
					"uninstall",
					"--keep-clients",
					"--root",
					root,
					"--json",
				],
				parsed.timeoutMs,
				true,
			);
			const detail = cleanup.stderr || cleanup.stdout || cleanup.error || "";
			if (
				cleanup.status !== 0 &&
				!(parsed.allowManagerSkip && managerUnavailable(detail))
			) {
				cleanupError = new Error(`autostart proof cleanup failed: ${detail}`);
			}
		}
		if (!parsed.keepTemp) fs.rmSync(tempDir, { recursive: true, force: true });
		if (cleanupError) throw cleanupError;
	}
}

let parsed;
try {
	parsed = parseArgs(argv);
	if (parsed.help) {
		process.stdout.write(`${usage()}\n`);
	} else {
		const report = await proof(parsed);
		if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
		else process.stdout.write(`${report.status.toUpperCase()} autostart lifecycle proof (${report.platform}/${report.arch})\n`);
		if (report.status === "skipped") process.exitCode = 0;
	}
} catch (error) {
	const report = {
		schema: "mcpace.autostartLifecycleProof.v1",
		status: "failed",
		error: error?.message ?? String(error),
	};
	if (parsed?.json || argv.includes("--json"))
		process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
	else process.stderr.write(`${report.error}\n`);
	process.exitCode = 1;
}
