# CLI npm Release

This project publishes the lightweight CLI as one main npm package plus one
native package per supported platform.

## Packages

- `@mtswitch/switch`: user-facing package. Installs the `matpool` command.
- `@mtswitch/switch-darwin-arm64`: native binary for macOS Apple Silicon.
- `@mtswitch/switch-darwin-x64`: native binary for macOS Intel.
- `@mtswitch/switch-linux-arm64`: native binary for Linux arm64.
- `@mtswitch/switch-linux-x64`: native binary for Linux x64.
- `@mtswitch/switch-win32-x64`: native binary for Windows x64.

The main package contains only a Node.js shim. npm installs the matching
optional native package for the current OS and CPU.

The GitHub Actions workflow builds macOS Apple Silicon on `macos-14` and macOS
Intel on `macos-15-intel`.

## Prerequisites

1. Create the npm organization/package scope `@matpool`.
2. Create an npm automation token with publish permission.
3. Add the token to the GitHub repository secret `NPM_TOKEN`.
4. Ensure package versions are identical across `packages/switch*`.
5. Confirm the version does not already exist on npm.

No Apple Developer certificate is required for `npm i -g @mtswitch/switch`.
The npm CLI package ships a command-line binary, not a macOS `.app` bundle or
`.pkg` installer. Apple signing/notarization is still needed for the desktop
client distribution.

## Preflight

Run the local metadata checks before creating a release tag:

```bash
pnpm release:cli:check
```

If network access is available, also verify that the package version is still
unpublished on npm:

```bash
pnpm release:cli:check:registry
```

The GitHub Actions release workflow runs the registry check before building any
native package. This avoids partially publishing platform packages when a
version has already been used.

## Local Validation

Build and run the CLI from the local Rust target:

```bash
cargo build --manifest-path src-tauri/Cargo.toml --bin matpool --no-default-features
node packages/switch/bin/matpool.js --version
node packages/switch/bin/matpool.js status
```

The plain developer build should also stay green for issue/QA compatibility,
although npm release artifacts use the headless build above:

```bash
cargo build --manifest-path src-tauri/Cargo.toml --bin matpool
```

Stage the current platform binary into its native npm package:

```bash
cargo build --manifest-path src-tauri/Cargo.toml --bin matpool --no-default-features --profile cli-release
node scripts/stage-npm-binary.js
node scripts/validate-cli-npm-release.js --require-staged-binaries
```

For debug builds:

```bash
node scripts/stage-npm-binary.js --debug
```

Do not commit staged native binaries from local validation.

Recommended release-candidate smoke test on each supported platform:

```bash
npm pack packages/switch
npm pack packages/<native-package-for-platform>
npm i -g ./matpool-switch-*.tgz ./matpool-switch-*.tgz
matpool provider seed
matpool models sync all
matpool setup --skip-login --skip-daemon --skip-takeover
matpool doctor
```

For a real end-to-end test, use an isolated OS user or temporary home directory,
then run:

```bash
matpool login --token <test_token>
matpool daemon run
matpool takeover codex
codex exec --skip-git-repo-check --cd <tmp> --json "Reply with exactly one word: pong"
matpool takeover all --disable
```

Keep the daemon running while the tool command executes. The CLI proxy is local;
takeover does not work when neither the daemon nor the desktop app is running.

## Publish

The GitHub Actions workflow `.github/workflows/release-cli-npm.yml` publishes
the packages in this order:

1. Validate package metadata and ensure the version is not already on npm.
2. Build each native package on its own platform runner.
3. Publish all native optional packages.
4. Publish the main `@mtswitch/switch` package.

Create a release tag:

```bash
git tag cli-v0.1.0
git push origin cli-v0.1.0
```

The tag version should match the package version.

For the first public rollout, prefer publishing an RC or the `next` dist-tag
first, validate on real macOS, Windows, and Linux machines, then promote to
`latest`.

## User Install

```bash
npm i -g @mtswitch/switch
matpool setup
matpool login
matpool status
matpool update
```

`matpool setup` is the recommended first command because it can configure login,
daemon startup, local proxy health checks, and takeover for supported apps in a
single flow.

## Release Checklist

- `pnpm release:cli:check`
- `pnpm release:cli:check:registry`
- `pnpm typecheck`
- `pnpm test:unit`
- `cargo check --manifest-path src-tauri/Cargo.toml --bin matpool --no-default-features`
- `cargo check --manifest-path src-tauri/Cargo.toml --bin matpool`
- `cargo check --manifest-path src-tauri/Cargo.toml --bin matpool-switch`
- Platform smoke tests for macOS, Windows, and Linux
- Real `codex exec`, Claude, and Gemini takeover smoke tests with a test token
