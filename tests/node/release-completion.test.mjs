import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import test from "node:test";
import { repoRoot } from "../../scripts/lib/project-metadata.mjs";

function read(relativePath) {
	return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function escapeRegExp(value) {
	return String(value).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function reportPath(value) {
	return path.isAbsolute(value) ? value : path.join(repoRoot, value);
}

function parseJson(value, label) {
	try {
		return JSON.parse(value);
	} catch (error) {
		assert.fail(`${label} is not valid JSON: ${error?.message ?? error}`);
	}
}

test("native npm package builder exists as the missing final publish lane", () => {
	const script = read("scripts/build-native-npm-package.mjs");
	assert.match(script, /mcpace\.nativeNpmPackageBuild\.v1/);
	assert.match(script, /validateBinaryForTarget/);
	assert.match(script, /copyRegularFileNoFollowSync/);
	assert.match(script, /["']npm["'],\s*\[\s*["']pack["']/);
	assert.match(script, /mcpace:\s*\{/);
	assert.match(script, /releaseSha/);
	assert.match(script, /MCPACE_RELEASE_SHA/);
	assert.match(script, /must be a full 40-character commit SHA/);
	assert.match(script, /rustTarget/);
	assert.match(script, /NOTICE/);

	const result = spawnSync(
		process.execPath,
		["scripts/build-native-npm-package.mjs", "--json"],
		{
			cwd: repoRoot,
			encoding: "utf8",
			windowsHide: true,
		},
	);
	assert.notEqual(
		result.status,
		0,
		"builder must fail closed without explicit target and binary",
	);
	const report = parseJson(result.stdout, "native package builder output");
	assert.equal(report.status, "failed");
});

test("installed runtime smoke requires an explicit binary and fails closed", () => {
	const result = spawnSync(
		process.execPath,
		["scripts/installer-runtime-smoke.mjs", "--json"],
		{
			cwd: repoRoot,
			encoding: "utf8",
			windowsHide: true,
		},
	);
	assert.notEqual(result.status, 0);
	const report = parseJson(result.stdout, "installer smoke output");
	assert.equal(report.schema, "mcpace.installerRuntimeSmoke.v1");
	assert.equal(report.status, "failed");
	assert.match(report.error, /--binary is required/);
});

test("installed runtime smoke retains bounded detached-start diagnostics", () => {
	const script = read("scripts/installer-runtime-smoke.mjs");
	assert.match(script, /function readBoundedTail/);
	assert.match(script, /serve stderr tail/);
	assert.match(script, /serve stdout tail/);
	assert.match(script, /serveLogDiagnostics\(root\)/);
	assert.match(
		script,
		/env:\s*childEnvForCommand\(command\)/,
		"the isolated installer root must not inherit developer MCPACE_* overrides",
	);
});

test("up checks the raw readiness contract instead of the grouped doctor report", () => {
	const setup = read("src/setup.rs");
	assert.match(
		setup,
		/"advanced"\.to_string\(\),\s*"doctor"\.to_string\(\),\s*"readiness"\.to_string\(\),\s*"--json"\.to_string\(\)/,
	);
});

test("native npm install smoke requires all standard-install inputs and fails closed", () => {
	const result = spawnSync(
		process.execPath,
		["scripts/native-npm-install-smoke.mjs", "--json"],
		{
			cwd: repoRoot,
			encoding: "utf8",
			windowsHide: true,
		},
	);
	assert.notEqual(result.status, 0);
	const report = parseJson(result.stdout, "native npm smoke output");
	assert.equal(report.schema, "mcpace.nativeNpmInstallSmoke.v1");
	assert.equal(report.status, "failed");
	assert.match(report.error, /--target is required/);
});

function createBinaryFixture(tmp, name, contents = "fixture binary\n") {
	const binaryPath = path.join(tmp, name);
	fs.writeFileSync(binaryPath, contents);
	try {
		fs.chmodSync(binaryPath, 0o755);
	} catch {
		// Windows filesystems may ignore POSIX execute bits.
	}
	return binaryPath;
}

function trimNullPaddedAscii(buffer, start, length) {
	return buffer
		.subarray(start, start + length)
		.toString("utf8")
		.replace(/\0.*$/s, "")
		.trim();
}

function tarOctal(buffer, start, length) {
	const raw = trimNullPaddedAscii(buffer, start, length).replace(/\s/g, "");
	return raw ? Number.parseInt(raw, 8) : 0;
}

function listTarGzEntries(data) {
	const buffer = zlib.gunzipSync(data);
	const entries = [];
	let offset = 0;
	while (offset + 512 <= buffer.length) {
		const header = buffer.subarray(offset, offset + 512);
		if (header.every((byte) => byte === 0)) break;
		const name = trimNullPaddedAscii(header, 0, 100);
		const prefix = trimNullPaddedAscii(header, 345, 155);
		const fullName = prefix ? `${prefix}/${name}` : name;
		const mode = tarOctal(header, 100, 8);
		const size = tarOctal(header, 124, 12);
		entries.push({ path: fullName, mode, size });
		offset += 512 + Math.ceil(size / 512) * 512;
	}
	return entries;
}

function readArMembers(filePath) {
	const buffer = fs.readFileSync(filePath);
	assert.equal(buffer.subarray(0, 8).toString("ascii"), "!<arch>\n");
	const members = [];
	let offset = 8;
	while (offset + 60 <= buffer.length) {
		const header = buffer.subarray(offset, offset + 60);
		const name = header
			.subarray(0, 16)
			.toString("ascii")
			.trim()
			.replace(/\/$/, "");
		const size = Number.parseInt(
			header.subarray(48, 58).toString("ascii").trim(),
			10,
		);
		assert.equal(header.subarray(58, 60).toString("ascii"), "`\n");
		const dataStart = offset + 60;
		const data = buffer.subarray(dataStart, dataStart + size);
		members.push({ name, data });
		offset = dataStart + size + (size % 2);
	}
	return members;
}

test("native GitHub installer builder emits installable package shapes", () => {
	const tmp = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-native-installer-"),
	);
	const outDir = path.join(tmp, "out");
	try {
		const winBinary = createBinaryFixture(tmp, "mcpace.exe", "MZ fixture\n");
		createBinaryFixture(
			tmp,
			"mcpace-agent-launcher.exe",
			"MZ hidden launcher fixture\n",
		);
		const win = spawnSync(
			process.execPath,
			[
				"scripts/build-native-installer-asset.mjs",
				"--target",
				"win32-x64-msvc",
				"--binary",
				winBinary,
				"--out-dir",
				outDir,
				"--wix-source-only",
				"--json",
			],
			{
				cwd: repoRoot,
				encoding: "utf8",
				windowsHide: true,
			},
		);
		assert.equal(win.status, 0, win.stderr || win.stdout);
		const winReport = JSON.parse(win.stdout);
		assert.equal(winReport.status, "pass");
		assert.equal(winReport.installerKind, "windows-msi");
		assert.match(winReport.installerName, /^mcpace-v.+-win32-x64-msvc\.msi$/);
		assert.equal(winReport.externalBuildSkipped, true);
		const wxs = fs.readFileSync(reportPath(winReport.wixSourcePath), "utf8");
		assert.match(wxs, /<Package Name="MCPace"/);
		assert.match(wxs, /<MajorUpgrade /);
		assert.match(wxs, /<Environment Id="MCPacePath" Name="PATH"/);
		assert.match(wxs, /Name="mcpace\.exe"/);
		assert.match(wxs, /MCPaceAgentLauncher/);
		assert.match(wxs, /Name="mcpace-agent-launcher\.exe"/);
		assert.deepEqual(
			winReport.sidecarBinaries.map((sidecar) => sidecar.name),
			["mcpace-agent-launcher.exe"],
		);
		assert.match(wxs, /MCPaceNotice/);
		assert.match(wxs, /Name="NOTICE"/);

		const linuxBinary = createBinaryFixture(tmp, "mcpace");
		const linux = spawnSync(
			process.execPath,
			[
				"scripts/build-native-installer-asset.mjs",
				"--target",
				"linux-x64-gnu",
				"--binary",
				linuxBinary,
				"--out-dir",
				outDir,
				"--json",
			],
			{
				cwd: repoRoot,
				encoding: "utf8",
				windowsHide: true,
			},
		);
		assert.equal(linux.status, 0, linux.stderr || linux.stdout);
		const linuxReport = JSON.parse(linux.stdout);
		assert.equal(linuxReport.status, "pass");
		assert.equal(linuxReport.installerKind, "debian-package");
		assert.match(linuxReport.installerName, /^mcpace-v.+-linux-x64-gnu\.deb$/);
		assert.match(linuxReport.sha256, /^[a-f0-9]{64}$/);
		const members = readArMembers(reportPath(linuxReport.installerPath));
		assert.deepEqual(
			members.map((member) => member.name),
			["debian-binary", "control.tar.gz", "data.tar.gz"],
		);
		assert.equal(members[0].data.toString("utf8"), "2.0\n");
		const control = zlib
			.gunzipSync(
				members.find((member) => member.name === "control.tar.gz").data,
			)
			.toString("utf8");
		assert.match(control, /Package: mcpace/);
		assert.match(control, /Architecture: amd64/);
		const dataEntries = listTarGzEntries(
			members.find((member) => member.name === "data.tar.gz").data,
		);
		const binaryEntry = dataEntries.find(
			(entry) => entry.path === "usr/bin/mcpace",
		);
		assert.ok(binaryEntry, "debian package missing /usr/bin/mcpace");
		assert.notEqual(
			binaryEntry.mode & 0o111,
			0,
			"installed linux binary must be executable",
		);
		assert.ok(
			dataEntries.some(
				(entry) => entry.path === "usr/share/doc/mcpace/LICENSE",
			),
		);
		assert.ok(
			dataEntries.some((entry) => entry.path === "usr/share/doc/mcpace/NOTICE"),
		);
	} finally {
		fs.rmSync(tmp, { recursive: true, force: true });
	}
});

test("release index writes checksums and machine-readable update metadata for every enabled installer", () => {
	const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "mcpace-release-index-"));
	try {
		const releaseTargets = JSON.parse(read("release-targets.json"));
		const enabledTargets = releaseTargets.targets.filter(
			(entry) => entry.publishEnabled !== false,
		);
		for (const target of enabledTargets) {
			const extension =
				target.platform === "win32"
					? "msi"
					: target.platform === "darwin"
						? "pkg"
						: "deb";
			fs.writeFileSync(
				path.join(tmp, `mcpace-v0.0.0-${target.key}.${extension}`),
				`${target.key}\n`,
			);
		}
		const result = spawnSync(
			process.execPath,
			[
				"scripts/build-release-index.mjs",
				"--asset-dir",
				tmp,
				"--out-dir",
				tmp,
				"--json",
			],
			{
				cwd: repoRoot,
				encoding: "utf8",
				windowsHide: true,
			},
		);
		assert.equal(result.status, 0, result.stderr || result.stdout);
		const report = JSON.parse(result.stdout);
		assert.equal(report.status, "pass");
		assert.equal(report.missingNativeInstallerTargets.length, 0);
		assert.equal(
			report.assets.filter((asset) => asset.target).length,
			enabledTargets.length,
		);
		const manifest = JSON.parse(
			fs.readFileSync(reportPath(report.manifestPath), "utf8"),
		);
		assert.equal(manifest.updatePolicy.mode, "package-manager-managed");
		assert.equal(
			manifest.updatePolicy.directGitHubInstallers,
			"manual-upgrade-only",
		);
		assert.equal(manifest.updatePolicy.selfRewrite, false);
		assert.match(
			manifest.updatePolicy.futureOsUpdateChannels.linux,
			/signed apt repository/,
		);
		assert.match(
			manifest.updatePolicy.futureOsUpdateChannels.windows,
			/winget/,
		);
		assert.match(
			manifest.updatePolicy.futureOsUpdateChannels.macos,
			/notarized PKG/,
		);
		assert.equal(manifest.ubuntu.packageFormat, "deb");
		assert.equal(manifest.ubuntu.glibcBaselineImage, "ubuntu:22.04");
		assert.equal(manifest.ubuntu.x64, "linux-x64-gnu");
		assert.equal(manifest.ubuntu.arm64, "linux-arm64-gnu");
		assert.ok(
			manifest.assets.some(
				(asset) =>
					asset.name.endsWith(".msi") &&
					asset.target.installerKind === "windows-msi",
			),
		);
		assert.ok(
			manifest.assets.some(
				(asset) =>
					asset.name.endsWith(".deb") &&
					asset.target.installCommand.includes("apt install"),
			),
		);
		assert.ok(
			manifest.assets.some(
				(asset) =>
					asset.name.endsWith(".deb") &&
					asset.target.glibcBaselineImage === "ubuntu:22.04",
			),
		);
		const checksums = fs.readFileSync(reportPath(report.checksumsPath), "utf8");
		assert.match(checksums, /mcpace-v0\.0\.0-linux-x64-gnu\.deb/);
		assert.match(checksums, /mcpace-v0\.0\.0-win32-x64-msvc\.msi/);
	} finally {
		fs.rmSync(tmp, { recursive: true, force: true });
	}
});

test("release index fails closed when any enabled installer is missing", () => {
	const tmp = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-release-index-missing-"),
	);
	try {
		const releaseTargets = JSON.parse(read("release-targets.json"));
		const enabledTargets = releaseTargets.targets.filter(
			(entry) => entry.publishEnabled !== false,
		);
		const [omittedTarget, ...includedTargets] = enabledTargets;
		for (const target of includedTargets) {
			const extension =
				target.platform === "win32"
					? "msi"
					: target.platform === "darwin"
						? "pkg"
						: "deb";
			fs.writeFileSync(
				path.join(tmp, `mcpace-v0.0.0-${target.key}.${extension}`),
				`${target.key}\n`,
			);
		}
		const result = spawnSync(
			process.execPath,
			[
				"scripts/build-release-index.mjs",
				"--asset-dir",
				tmp,
				"--out-dir",
				tmp,
				"--json",
			],
			{
				cwd: repoRoot,
				encoding: "utf8",
				windowsHide: true,
			},
		);
		assert.notEqual(
			result.status,
			0,
			"release index must fail when an enabled installer is missing",
		);
		const report = JSON.parse(result.stdout);
		assert.equal(report.status, "failed");
		assert.match(
			report.error,
			new RegExp(
				`missing native GitHub release installers for targets: ${escapeRegExp(omittedTarget.key)}`,
			),
		);
	} finally {
		fs.rmSync(tmp, { recursive: true, force: true });
	}
});

test("publish workflow builds native packages before publishing the main launcher", () => {
	const workflow = read(".github/workflows/publish-npm.yml");
	assert.match(workflow, /native-packages:/);
	assert.match(
		workflow,
		/needs:\s*\n\s*-\s*publish-plan\s*\n\s*-\s*native-packages/,
	);
	assert.match(workflow, /cargo fmt --check/);
	assert.doesNotMatch(workflow, /npm run proof:rust-live:enforce/);
	assert.match(workflow, /npm run check:ci/);
	assert.match(workflow, /cargo clippy --locked --all-targets\s+--target/);
	assert.match(
		workflow,
		/cargo test --locked --target[^\n]*\s+-- --test-threads=1/,
	);
	assert.match(
		workflow,
		/Build and test Linux native binary in glibc baseline container/,
	);
	assert.match(workflow, /bash scripts\/build-linux-glibc-baseline\.sh/);
	assert.match(workflow, /MCPACE_GLIBC_BASELINE_CHECKS: full/);
	assert.match(workflow, /glibc_baseline_image: ubuntu:22\.04/);
	assert.match(
		workflow,
		/node scripts\/build-native-npm-package\.mjs\s+\\\s+--target/,
	);
	assert.match(workflow, /Verify standard launcher package runtime/);
	assert.match(workflow, /check:native-npm-install-smoke/);
	assert.match(workflow, /tarballs=\(dist\/npm\/\*\.tgz\)/);
	const npmInstallSmoke = read("scripts/native-npm-install-smoke.mjs");
	assert.match(npmInstallSmoke, /--offline/);
	assert.match(npmInstallSmoke, /registry.*unreachable/);
	assert.match(npmInstallSmoke, /resolvedPrefix = fs\.realpathSync\(appDir\)/);
	assert.match(npmInstallSmoke, /pathInside\(resolvedPrefix, resolved\)/);
	assert.match(npmInstallSmoke, /invalid installed package JSON/);
	const linuxBaseline = read("scripts/build-linux-glibc-baseline.sh");
	assert.match(
		linuxBaseline,
		/cargo metadata --locked --format-version 1 --no-deps >\/dev\/null/,
	);
	assert.doesNotMatch(linuxBaseline, /cargo generate-lockfile --locked/);
	assert.match(workflow, /expected 6 native npm tarballs/);
	assert.match(
		workflow,
		/node scripts\/verify-npm-publish-contract\.mjs --enforce/,
	);
	assert.match(workflow, /registry_has_matching_release/);
	assert.match(workflow, /mcpace\?\.releaseSha/);
	assert.match(workflow, /refusing mixed-SHA resume/);
	assert.match(workflow, /already published; skipping \$package_spec/);
	assert.ok(
		workflow.indexOf("Publish or resume native npm package set") <
			workflow.indexOf("Publish main npm launcher last"),
	);
});

test("release workflow builds platform installers and checksummed draft GitHub release assets", () => {
	const workflow = read(".github/workflows/release.yml");
	assert.match(workflow, /native-release-assets:/);
	assert.match(workflow, /native GitHub installer/);
	assert.match(workflow, /compose-release-assets:/);
	assert.match(
		workflow,
		/dotnet tool install --global wix --version 5\.0\.2 --configfile \.github[\\/]nuget-wix\.config/,
	);
	assert.match(workflow, /\^5\\\.0\\\.2\(\?:\\\+\|\$\)/);
	assert.doesNotMatch(workflow, /AcceptEula|WIXTOOLSET_WIX_EULA/);
	const wixNugetConfig = read(".github/nuget-wix.config");
	assert.match(wixNugetConfig, /<clear\s*\/>/);
	assert.match(
		wixNugetConfig,
		/https:\/\/api\.nuget\.org\/v3\/index\.json/,
	);
	assert.match(
		workflow,
		/Build Linux native binary in glibc baseline container/,
	);
	assert.match(workflow, /bash scripts\/build-linux-glibc-baseline\.sh/);
	assert.match(workflow, /MCPACE_GLIBC_BASELINE_CHECKS: build/);
	assert.match(workflow, /glibc_baseline_image: ubuntu:22\.04/);
	assert.match(
		workflow,
		/node scripts\/build-native-installer-asset\.mjs\s+\\?\s*--target/,
	);
	assert.match(workflow, /pattern: native-release-\*/);
	assert.match(workflow, /Verify Ubuntu\/Debian installer/);
	assert.match(workflow, /Verify Windows MSI installer/);
	assert.match(workflow, /System32\\msiexec\.exe/);
	assert.match(workflow, /-FilePath \$msiexec/);
	assert.match(workflow, /\/L\*V/);
	assert.match(workflow, /PassThru/);
	assert.match(workflow, /msiexec failed/);
	assert.match(workflow, /Verify macOS native architecture and dependencies/);
	assert.match(workflow, /Verify macOS PKG installer, receipt, and runtime/);
	assert.match(workflow, /lipo -archs/);
	assert.match(workflow, /pkgutil --pkg-info io\.github\.ramenm\.mcpace/);
	assert.match(
		workflow,
		/npm run check:installer-runtime-smoke --\s+[\\`]?\s*--binary/,
	);
	assert.match(workflow, /npm run platform:binary-smoke -- --binary\s+"target/);
	assert.match(workflow, /actions\/attest@[0-9a-f]{40}\s+# v4/);
	assert.match(
		workflow,
		/node scripts\/build-release-index\.mjs --json\s+--asset-dir dist\/release-assets\s+--out-dir dist\/release-assets/,
	);
	assert.match(workflow, /Attest composed GitHub release asset set/);
	assert.match(workflow, /subject-path: dist\/release-assets\/\*/);
	assert.match(workflow, /name: github-release-assets/);
	assert.match(
		workflow,
		/mcpace-v.+-checksums\.sha256|checksums and release asset manifest/,
	);
	assert.match(workflow, /fetch-depth: 0/);
	assert.match(
		workflow,
		/git rev-parse "refs\/tags\/\$release_tag\^\{commit\}"/,
	);
	assert.match(workflow, /tag_sha.*MCPACE_BUILD_SHA/s);
	assert.match(workflow, /release tag.*package version/s);
	assert.match(
		workflow,
		/if \[ "\$MCPACE_CREATE_RELEASE" = "true" \]; then\s+release_requested=true/,
	);
	assert.doesNotMatch(
		workflow,
		/if \[ "\$MCPACE_REF_TYPE" = "tag" \]; then\s+release_requested=true/,
	);
	assert.match(workflow, /environment: github-release/);
	assert.match(workflow, /EXPECTED_RELEASE_SHA/);
	assert.match(workflow, /git fetch --force --no-tags origin/);
	assert.ok((workflow.match(/assert_tag_binding/g) || []).length >= 4);
	assert.match(
		workflow,
		/done\s+assert_tag_binding\s+assert_release_is_draft/s,
	);
	assert.match(workflow, /assert_release_is_draft/);
	assert.match(workflow, /refusing to mutate published release/);
	assert.match(workflow, /unexpected draft asset/);
	assert.match(workflow, /draft asset digest mismatch/);
	assert.match(workflow, /identical draft asset exists; skipping/);
	assert.match(workflow, /group:\s*>-\s+release-\$\{\{/);
	assert.match(workflow, /\[string\]::Equals\(/);
	assert.match(workflow, /OrdinalIgnoreCase/);
	assert.match(workflow, /gh release upload "\$RELEASE_TAG" "\$asset"/);
	assert.doesNotMatch(workflow, /--clobber/);
});

test("release workflow native asset matrix mirrors release target metadata", () => {
	const workflow = read(".github/workflows/release.yml");
	const releaseTargets = parseJson(
		read("release-targets.json"),
		"release targets",
	);
	const enabledTargets = releaseTargets.targets.filter(
		(entry) => entry.publishEnabled !== false,
	);
	const plannedTargets = releaseTargets.plannedTargets.filter(
		(entry) => entry.publishEnabled === false,
	);
	for (const target of enabledTargets) {
		assert.match(workflow, new RegExp(`- key: ${escapeRegExp(target.key)}\n`));
		assert.match(
			workflow,
			new RegExp(`platform: ${escapeRegExp(target.platform)}`),
		);
		assert.match(
			workflow,
			new RegExp(`rust_target: ${escapeRegExp(target.rustTarget)}`),
		);
		assert.match(
			workflow,
			new RegExp(`binary_name: ${escapeRegExp(target.binaryName)}`),
		);
		if (target.glibcBaselineImage) {
			assert.match(
				workflow,
				new RegExp(
					`glibc_baseline_image: ${escapeRegExp(target.glibcBaselineImage)}`,
				),
			);
		}
	}
	for (const target of plannedTargets) {
		assert.doesNotMatch(
			workflow,
			new RegExp(`- key: ${escapeRegExp(target.key)}\n`),
			`${target.key} must stay out of native release matrix until enabled`,
		);
	}
});

test("publish workflow native matrix mirrors release target metadata", () => {
	const workflow = read(".github/workflows/publish-npm.yml");
	const releaseTargets = parseJson(
		read("release-targets.json"),
		"release targets",
	);
	const enabledTargets = releaseTargets.targets.filter(
		(entry) => entry.publishEnabled !== false,
	);
	for (const target of enabledTargets) {
		assert.match(workflow, new RegExp(`- key: ${escapeRegExp(target.key)}\n`));
		assert.match(
			workflow,
			new RegExp(`platform: ${escapeRegExp(target.platform)}`),
		);
		assert.match(
			workflow,
			new RegExp(`rust_target: ${escapeRegExp(target.rustTarget)}`),
		);
		assert.match(
			workflow,
			new RegExp(`binary_name: ${escapeRegExp(target.binaryName)}`),
		);
		if (target.glibcBaselineImage) {
			assert.match(
				workflow,
				new RegExp(
					`glibc_baseline_image: ${escapeRegExp(target.glibcBaselineImage)}`,
				),
			);
		}
	}
});

test("release completion documentation and scripts are part of the source release", () => {
	const manifest = parseJson(read("release-manifest.json"), "release manifest");
	assert.ok(manifest.includePaths.includes("docs/release-completion.md"));
	assert.ok(manifest.includePaths.includes("docs/signing-and-notarization.md"));
	assert.ok(manifest.includePaths.includes("NOTICE"));
	assert.ok(
		manifest.includePaths.includes("scripts/build-native-npm-package.mjs"),
	);
	assert.ok(
		manifest.includePaths.includes("scripts/build-native-installer-asset.mjs"),
	);
	assert.ok(
		manifest.includePaths.includes("scripts/build-linux-glibc-baseline.sh"),
	);
	assert.ok(
		manifest.includePaths.includes("scripts/installer-runtime-smoke.mjs"),
	);
	assert.ok(
		manifest.includePaths.includes("scripts/native-npm-install-smoke.mjs"),
	);
	assert.ok(manifest.includePaths.includes("scripts/build-release-index.mjs"));
	assert.ok(
		!manifest.includePaths.includes("scripts/build-native-release-asset.mjs"),
	);
	const docs = read("docs/release-completion.md");
	assert.match(
		docs,
		/A tarball that merely has the right filename is not enough/,
	);
	assert.match(docs, /npm trusted publishers are configured/);
	assert.match(docs, /source-code archives/);
	assert.match(docs, /Windows users (?:will )?install/);
	assert.match(docs, /Ubuntu users (?:will )?install/);
	assert.match(docs, /Current public lane: npm only/);
	assert.match(docs, /private draft proofs/);
	assert.match(docs, /manual-upgrade-only/);
	assert.match(docs, /signed apt repository/);
	assert.match(docs, /Authenticode-sign/);
	assert.match(docs, /notarize with Apple/);
	assert.match(docs, /docs\/signing-and-notarization\.md/);
	assert.match(docs, /full-length commit SHAs/);
	assert.match(docs, /Third-party notices/);
	assert.match(docs, /Apache-2\.0/);
	assert.match(docs, /Copyright 2026 Ramenm/);
	assert.match(docs, /ubuntu:22\.04/);
	assert.match(docs, /Ubuntu 24\.04/);
	assert.match(docs, /Installed-runtime verification/);
	assert.match(docs, /lipo -archs/);
	assert.match(docs, /pkgutil --pkg-info io\.github\.ramenm\.mcpace/);
	assert.match(docs, /native-npm-install-smoke\.mjs/);
	assert.match(docs, /registry deliberately unreachable/);
	const rootReadme = read("README.md");
	assert.match(rootReadme, /npm install -g @mcpace\/cli@latest/);
	assert.match(rootReadme, /Copyright 2026 Ramenm/);
	const npmReadme = read("packages/npm/cli/README.md");
	assert.match(npmReadme, /npm install -g @mcpace\/cli@latest/);
	assert.match(npmReadme, /Apache-2\.0/);
	const signingRunbook = read("docs/signing-and-notarization.md");
	assert.match(signingRunbook, /Microsoft Artifact Signing/);
	assert.match(signingRunbook, /GitHub Actions OIDC/);
	assert.match(signingRunbook, /Developer ID Application/);
	assert.match(signingRunbook, /Developer ID Installer/);
	assert.match(signingRunbook, /notarytool submit --wait/);
	assert.match(signingRunbook, /stapler staple/);
	assert.match(signingRunbook, /Get-AuthenticodeSignature/);
	assert.match(signingRunbook, /windows-11-arm/);
});
