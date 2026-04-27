// Generated from release-targets.json by scripts/sync-platform-packages.mjs.
// Do not edit by hand.
export const RELEASE_TARGETS = [
  {
    "publishEnabled": true,
    "key": "linux-x64-gnu",
    "platform": "linux",
    "arch": "x64",
    "libcProbe": "gnu",
    "triple": "x86_64-unknown-linux-gnu",
    "rustTarget": "x86_64-unknown-linux-gnu",
    "runner": "ubuntu-24.04",
    "packageName": "@mcpace/cli-linux-x64-gnu",
    "npmPackage": "@mcpace/cli-linux-x64-gnu",
    "binaryName": "mcpace",
    "os": [
      "linux"
    ],
    "cpu": [
      "x64"
    ],
    "libc": [
      "glibc"
    ],
    "nodePlatform": "linux",
    "nodeArch": "x64"
  },
  {
    "publishEnabled": true,
    "key": "linux-arm64-gnu",
    "platform": "linux",
    "arch": "arm64",
    "libcProbe": "gnu",
    "triple": "aarch64-unknown-linux-gnu",
    "rustTarget": "aarch64-unknown-linux-gnu",
    "runner": "ubuntu-24.04-arm",
    "packageName": "@mcpace/cli-linux-arm64-gnu",
    "npmPackage": "@mcpace/cli-linux-arm64-gnu",
    "binaryName": "mcpace",
    "os": [
      "linux"
    ],
    "cpu": [
      "arm64"
    ],
    "libc": [
      "glibc"
    ],
    "nodePlatform": "linux",
    "nodeArch": "arm64"
  },
  {
    "publishEnabled": true,
    "key": "darwin-x64",
    "platform": "darwin",
    "arch": "x64",
    "triple": "x86_64-apple-darwin",
    "rustTarget": "x86_64-apple-darwin",
    "runner": "macos-15-intel",
    "packageName": "@mcpace/cli-darwin-x64",
    "npmPackage": "@mcpace/cli-darwin-x64",
    "binaryName": "mcpace",
    "os": [
      "darwin"
    ],
    "cpu": [
      "x64"
    ],
    "nodePlatform": "darwin",
    "nodeArch": "x64"
  },
  {
    "publishEnabled": true,
    "key": "darwin-arm64",
    "platform": "darwin",
    "arch": "arm64",
    "triple": "aarch64-apple-darwin",
    "rustTarget": "aarch64-apple-darwin",
    "runner": "macos-15",
    "packageName": "@mcpace/cli-darwin-arm64",
    "npmPackage": "@mcpace/cli-darwin-arm64",
    "binaryName": "mcpace",
    "os": [
      "darwin"
    ],
    "cpu": [
      "arm64"
    ],
    "nodePlatform": "darwin",
    "nodeArch": "arm64"
  },
  {
    "publishEnabled": true,
    "key": "win32-x64-msvc",
    "platform": "win32",
    "arch": "x64",
    "triple": "x86_64-pc-windows-msvc",
    "rustTarget": "x86_64-pc-windows-msvc",
    "runner": "windows-2025",
    "packageName": "@mcpace/cli-win32-x64-msvc",
    "npmPackage": "@mcpace/cli-win32-x64-msvc",
    "binaryName": "mcpace.exe",
    "os": [
      "win32"
    ],
    "cpu": [
      "x64"
    ],
    "nodePlatform": "win32",
    "nodeArch": "x64"
  },
  {
    "publishEnabled": true,
    "key": "win32-arm64-msvc",
    "platform": "win32",
    "arch": "arm64",
    "triple": "aarch64-pc-windows-msvc",
    "rustTarget": "aarch64-pc-windows-msvc",
    "runner": "windows-11-arm",
    "packageName": "@mcpace/cli-win32-arm64-msvc",
    "npmPackage": "@mcpace/cli-win32-arm64-msvc",
    "binaryName": "mcpace.exe",
    "os": [
      "win32"
    ],
    "cpu": [
      "arm64"
    ],
    "nodePlatform": "win32",
    "nodeArch": "arm64"
  },
  {
    "publishEnabled": false,
    "key": "linux-x64-musl",
    "platform": "linux",
    "arch": "x64",
    "libcProbe": "musl",
    "triple": "x86_64-unknown-linux-musl",
    "rustTarget": "x86_64-unknown-linux-musl",
    "runner": "ubuntu-24.04",
    "packageName": "@mcpace/cli-linux-x64-musl",
    "npmPackage": "@mcpace/cli-linux-x64-musl",
    "binaryName": "mcpace",
    "os": [
      "linux"
    ],
    "cpu": [
      "x64"
    ],
    "libc": [
      "musl"
    ],
    "reason": "Requires a dedicated Alpine/musl build and install proof before publication.",
    "nodePlatform": "linux",
    "nodeArch": "x64"
  },
  {
    "publishEnabled": false,
    "key": "linux-arm64-musl",
    "platform": "linux",
    "arch": "arm64",
    "libcProbe": "musl",
    "triple": "aarch64-unknown-linux-musl",
    "rustTarget": "aarch64-unknown-linux-musl",
    "runner": "ubuntu-24.04-arm",
    "packageName": "@mcpace/cli-linux-arm64-musl",
    "npmPackage": "@mcpace/cli-linux-arm64-musl",
    "binaryName": "mcpace",
    "os": [
      "linux"
    ],
    "cpu": [
      "arm64"
    ],
    "libc": [
      "musl"
    ],
    "reason": "Requires a dedicated Alpine/musl build and install proof before publication.",
    "nodePlatform": "linux",
    "nodeArch": "arm64"
  }
];

export const SUPPORTED_TARGETS = RELEASE_TARGETS.filter((target) => target.publishEnabled !== false);

export const PLANNED_TARGETS = RELEASE_TARGETS.filter((target) => target.publishEnabled === false);
