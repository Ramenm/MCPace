import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const PROVENANCE_MODULE_PATH = fileURLToPath(import.meta.url);

const ROOT_FILES = [
	"Cargo.lock",
	"Cargo.toml",
	"rust-toolchain.toml",
	"catalog/approved-servers.json",
];
const OPTIONAL_ROOTS = [".cargo", "build.rs"];

function toPosix(relativePath) {
	return relativePath.split(path.sep).join("/");
}

function collectRegularFiles(repoRoot, relativePath, files) {
	const absolutePath = path.join(repoRoot, relativePath);
	if (!fs.existsSync(absolutePath)) return;
	const entry = fs.lstatSync(absolutePath);
	if (entry.isSymbolicLink()) {
		throw new Error(
			`Rust build provenance input must not be a symlink: ${relativePath}`,
		);
	}
	if (entry.isFile()) {
		files.push(toPosix(relativePath));
		return;
	}
	if (!entry.isDirectory()) {
		throw new Error(
			`Rust build provenance input is not a regular file: ${relativePath}`,
		);
	}
	for (const child of fs.readdirSync(absolutePath, { withFileTypes: true })) {
		collectRegularFiles(repoRoot, path.join(relativePath, child.name), files);
	}
}

export function rustBuildInputFiles(repoRoot) {
	const files = [];
	for (const relativePath of [...ROOT_FILES, "src", ...OPTIONAL_ROOTS])
		collectRegularFiles(repoRoot, relativePath, files);
	return [...new Set(files)].sort((left, right) => left.localeCompare(right));
}

function openStatFingerprint(value) {
	return [value.dev, value.ino, value.mode, value.size, value.mtimeMs].join(
		":",
	);
}

function stableStatFingerprint(value) {
	return `${openStatFingerprint(value)}:${value.ctimeMs}`;
}

export function sha256File(filePath) {
	const linkStat = fs.lstatSync(filePath);
	if (linkStat.isSymbolicLink() || !linkStat.isFile()) {
		throw new Error(
			`SHA-256 input must be a regular non-symlink file: ${filePath}`,
		);
	}
	const descriptor = fs.openSync(
		filePath,
		fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW ?? 0),
	);
	try {
		const before = fs.fstatSync(descriptor);
		const content = fs.readFileSync(descriptor);
		const after = fs.fstatSync(descriptor);
		const pathAfter = fs.lstatSync(filePath);
		if (
			pathAfter.isSymbolicLink() ||
			!pathAfter.isFile() ||
			openStatFingerprint(before) !== openStatFingerprint(linkStat) ||
			stableStatFingerprint(after) !== stableStatFingerprint(before) ||
			openStatFingerprint(pathAfter) !== openStatFingerprint(after) ||
			content.length !== before.size
		) {
			throw new Error(`SHA-256 input changed while being read: ${filePath}`);
		}
		return crypto.createHash("sha256").update(content).digest("hex");
	} finally {
		fs.closeSync(descriptor);
	}
}

export function createVerifiedArtifactCopy(
	sourcePath,
	destinationPath,
	expectedSha256,
) {
	fs.copyFileSync(sourcePath, destinationPath, fs.constants.COPYFILE_EXCL);
	try {
		if (process.platform !== "win32") {
			fs.chmodSync(destinationPath, fs.statSync(sourcePath).mode & 0o777);
		}
		const actualSha256 = sha256File(destinationPath);
		if (actualSha256 !== expectedSha256) {
			throw new Error(
				"private execution artifact does not match the release artifact binding",
			);
		}
		return { path: destinationPath, sha256: actualSha256 };
	} catch (error) {
		fs.rmSync(destinationPath, { force: true });
		throw error;
	}
}

export function provenanceGeneratorSha256() {
	return sha256File(PROVENANCE_MODULE_PATH);
}

export function rustBuildProvenance(repoRoot) {
	const files = rustBuildInputFiles(repoRoot);
	const hash = crypto.createHash("sha256");
	const fileHashes = {};
	for (const relativePath of files) {
		const contents = fs.readFileSync(path.join(repoRoot, relativePath));
		const fileHash = crypto.createHash("sha256").update(contents).digest("hex");
		fileHashes[relativePath] = fileHash;
		hash.update(relativePath, "utf8");
		hash.update("\0");
		hash.update(fileHash, "ascii");
		hash.update("\0");
	}
	return {
		algorithm: "sha256",
		fingerprint: hash.digest("hex"),
		fileCount: files.length,
		fileHashes,
	};
}

export function releaseBinaryPath(repoRoot) {
	return path.join(
		repoRoot,
		"target",
		"release",
		process.platform === "win32" ? "mcpace.exe" : "mcpace",
	);
}

export function verifyRustProofRecord({
	repoRoot,
	report,
	proofGeneratorPath,
}) {
	const provenance = rustBuildProvenance(repoRoot);
	const expectedGenerator = sha256File(proofGeneratorPath);
	if (
		report?.schema !== "mcpace.rustLiveProof.v1" ||
		report.status !== "pass" ||
		report.blockers !== 0 ||
		report.releaseBuildExecuted !== true
	) {
		throw new Error(
			"Rust live proof does not contain a successful release build",
		);
	}
	if (report.proofGeneratorSha256 !== expectedGenerator) {
		throw new Error("Rust live proof predates the current proof generator");
	}
	if (report.provenanceGeneratorSha256 !== provenanceGeneratorSha256()) {
		throw new Error(
			"Rust live proof predates the current provenance generator",
		);
	}
	const expectedSnapshot = {
		sourceFingerprint: provenance.fingerprint,
		sourceFileCount: provenance.fileCount,
		proofGeneratorSha256: expectedGenerator,
		provenanceGeneratorSha256: provenanceGeneratorSha256(),
	};
	const snapshotsMatch = [
		report.proofInputSnapshots?.before,
		report.proofInputSnapshots?.after,
	].every(
		(snapshot) =>
			snapshot &&
			Object.entries(expectedSnapshot).every(
				([key, value]) => snapshot[key] === value,
			),
	);
	if (!snapshotsMatch) {
		throw new Error(
			"Rust live proof did not stabilize sources and proof generators across the run",
		);
	}
	if (
		report.rustBuildInputs?.fingerprint !== provenance.fingerprint ||
		report.rustBuildInputs?.fileCount !== provenance.fileCount
	) {
		throw new Error(
			"Rust live proof does not match the current Rust build inputs",
		);
	}
	const binarySha256 = report.releaseArtifact?.sha256;
	if (
		!/^[a-f0-9]{64}$/.test(binarySha256 || "") ||
		report.releaseArtifact?.sourceFingerprint !== provenance.fingerprint ||
		report.releaseArtifactStability?.stable !== true ||
		report.releaseArtifactStability?.beforeSha256 !== binarySha256 ||
		report.releaseArtifactStability?.afterSha256 !== binarySha256
	) {
		throw new Error(
			"Rust live proof release artifact record is not internally bound",
		);
	}
	return {
		binarySha256,
		provenance,
		rustBuildBinding: {
			generatedAt: report.generatedAt,
			proofGeneratorSha256: report.proofGeneratorSha256,
			provenanceGeneratorSha256: report.provenanceGeneratorSha256,
			releaseArtifact: report.releaseArtifact,
		},
	};
}

export function verifyRustProofBinding({
	repoRoot,
	binaryPath,
	report,
	proofGeneratorPath,
}) {
	const verified = verifyRustProofRecord({
		repoRoot,
		report,
		proofGeneratorPath,
	});
	if (sha256File(binaryPath) !== verified.binarySha256) {
		throw new Error(
			"selected release binary is not bound to the current Rust proof",
		);
	}
	return verified;
}
