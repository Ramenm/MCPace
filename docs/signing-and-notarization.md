# Windows and macOS signing runbook

This is the production signing plan for MCPace release artifacts. It intentionally separates local install proof from public trust: unsigned MSI/PKG files are useful for CI smoke tests, but public GitHub Release assets should not be published until the signing gates below are green.

## Recommended default

Use platform-native signing rather than a custom self-updater:

- Windows: sign `mcpace.exe` and the final `.msi` with Microsoft Artifact Signing (formerly Trusted Signing) or an equivalent HSM-backed public code-signing certificate, and timestamp every signature.
- macOS: sign the CLI binary with Developer ID Application, build/sign the package with Developer ID Installer, notarize the `.pkg` with Apple notary service, then staple and validate the notarization ticket.
- Linux: keep direct `.deb` downloads as manual installers; only advertise automatic OS upgrades after a signed apt repository exists.

Do not store a raw private signing key in the repository. Prefer cloud/HSM-backed signing and short-lived CI identity federation. If a P12/PFX file is temporarily used for macOS certificate import, keep it only in GitHub Actions secrets, import into a temporary keychain, and delete it at the end of the job.

## Simpler staged path

The fully signed path is the production target, not the first required release.
For a small CLI, keep the first public path simple:

1. **Easy default now: npm-first release.** Publish `@mcpace/cli` plus native optional packages. Users install with `npm install -g @mcpace/cli@latest`; updates also stay npm-managed. This avoids Windows MSI and macOS PKG trust setup for the first release.
2. **Linux manual installer is acceptable early.** A direct `.deb` is reasonable as a GitHub Release convenience asset because Linux package signing matters most when running a repository/auto-update channel. Do not advertise apt auto-updates until a signed apt repository exists.
3. **Keep Windows/macOS installers as draft or preview until signed.** Unsigned MSI/PKG files are useful for CI proof and technically installable, but should not be the recommended public path.
4. **Add one signing lane at a time.** Do Windows first if most users are on Windows; do macOS first only if macOS downloads become important. There is no need to build both signing systems before the npm-first release.

This staged plan keeps the release honest: npm is the supported installer/update path today; OS installers become recommended only after their platform trust gates are green.

## Windows plan

Best fit for this repository: Microsoft Artifact Signing via GitHub Actions OIDC.

Required external setup:

1. Paid Azure subscription and Microsoft Entra tenant.
2. Register the `Microsoft.CodeSigning` resource provider.
3. Create an Artifact Signing account.
4. Complete public-trust identity validation.
5. Create a certificate profile for public code signing.
6. Grant the GitHub Actions federated identity access to sign with that account/profile.

Repository configuration to add once the Azure resources exist:

- GitHub environment: `release-signing` with required reviewers.
- GitHub variables:
  - `MCPACE_WINDOWS_SIGNING_ENABLED=true`
  - `AZURE_ARTIFACT_SIGNING_ENDPOINT`
  - `AZURE_ARTIFACT_SIGNING_ACCOUNT`
  - `AZURE_ARTIFACT_SIGNING_CERT_PROFILE`
  - `AZURE_SUBSCRIPTION_ID`
  - `AZURE_TENANT_ID`
  - `AZURE_CLIENT_ID`
- No long-lived Azure client secret if OIDC federation is configured correctly.

Signing order:

1. Build `mcpace.exe` for `win32-x64-msvc` and `win32-arm64-msvc`.
2. Authenticode-sign each `mcpace.exe` with SHA-256 digest and RFC 3161 timestamp.
3. Build the MSI from the signed executable.
4. Authenticode-sign the MSI with SHA-256 digest and RFC 3161 timestamp.
5. Verify both signatures with `Get-AuthenticodeSignature` or `signtool verify /pa`.
6. Install the MSI on the target Windows runner, run `mcpace help`, and uninstall it.

Important ARM64 note: Azure's GitHub Artifact Signing action currently runs on standard Windows GitHub runners, not Windows ARM runners. For `win32-arm64-msvc`, sign on a Windows x64 signing runner after building/copying the ARM64 executable, then either rebuild the ARM64 MSI on that signing runner or run a follow-up install proof on `windows-11-arm` using the signed MSI.

Do not publish a Windows release asset when either the embedded `.exe` or the `.msi` is unsigned.

## macOS plan

Best fit for this repository: Apple Developer ID certificates plus `notarytool` in GitHub Actions.

Required external setup:

1. Apple Developer Program membership.
2. Developer ID Application certificate for signing the CLI binary.
3. Developer ID Installer certificate for signing the `.pkg`.
4. App Store Connect API key that can submit notarization requests.
5. A clean GitHub `release-signing` environment with required reviewers.

Repository configuration to add once credentials exist:

- GitHub variables:
  - `MCPACE_MACOS_SIGNING_ENABLED=true`
  - `MCPACE_MACOS_APPLICATION_IDENTITY` (for example `Developer ID Application: ...`)
  - `MCPACE_MACOS_INSTALLER_IDENTITY` (for example `Developer ID Installer: ...`)
  - `APPLE_NOTARY_KEY_ID`
  - `APPLE_NOTARY_ISSUER_ID`
- GitHub secrets:
  - `MACOS_DEVELOPER_ID_APPLICATION_CERT_P12_BASE64`
  - `MACOS_DEVELOPER_ID_APPLICATION_CERT_PASSWORD`
  - `MACOS_DEVELOPER_ID_INSTALLER_CERT_P12_BASE64`
  - `MACOS_DEVELOPER_ID_INSTALLER_CERT_PASSWORD`
  - `APPLE_NOTARY_KEY_P8_BASE64`

Signing order:

1. Import both P12 certificates into a temporary keychain.
2. Build the Rust binary on the macOS runner.
3. Sign the binary with Developer ID Application, hardened runtime, and timestamp:
   `codesign --force --options runtime --timestamp --sign "$MCPACE_MACOS_APPLICATION_IDENTITY" mcpace`.
4. Verify the binary with `codesign --verify --strict --verbose=2`.
5. Build/sign the `.pkg` with Developer ID Installer.
6. Submit the `.pkg` to Apple notarization with `xcrun notarytool submit --wait`.
7. Staple the notarization ticket with `xcrun stapler staple`.
8. Validate with `xcrun stapler validate`, `pkgutil --check-signature`, and `spctl --assess --type install`.
9. Install the notarized/stapled package, run `/usr/local/bin/mcpace help`, and remove it.
10. Delete the temporary keychain even on failure.

Do not publish a macOS `.pkg` that is only locally signed but not notarized and stapled.

## Go/no-go gate for public GitHub Releases

A public release is GO only if all of these are true:

- Linux `.deb` assets are built from the Ubuntu 22.04 glibc baseline and install on the oldest supported glibc distro.
- Windows `.exe` and `.msi` are both Authenticode-signed and timestamped for every Windows target.
- macOS binary is Developer ID Application signed; `.pkg` is Developer ID Installer signed, notarized, stapled, and install-tested.
- Checksums and `mcpace-v<version>-release-assets.json` are generated from the final signed assets, not from pre-signing files.
- GitHub artifact attestations exist for the final published assets.
- The release is still a draft until a human compares asset names, hashes, and install proof logs.
