#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');
const { spawnSync } = require('child_process');

function platformPackageName() {
  const platform = process.platform;
  const arch = process.arch;

  if (platform === 'darwin' && arch === 'arm64') return '@mtswitch/switch-darwin-arm64';
  if (platform === 'darwin' && arch === 'x64') return '@mtswitch/switch-darwin-x64';
  if (platform === 'linux' && arch === 'arm64') return '@mtswitch/switch-linux-arm64';
  if (platform === 'linux' && arch === 'x64') return '@mtswitch/switch-linux-x64';
  if (platform === 'win32' && arch === 'x64') return '@mtswitch/switch-win32-x64';

  throw new Error(`Unsupported platform: ${platform}-${arch}`);
}

function nativeBinaryName() {
  return process.platform === 'win32' ? 'matpool.exe' : 'matpool';
}

function resolveInstalledBinary() {
  const packageName = platformPackageName();
  const packageJsonPath = require.resolve(`${packageName}/package.json`);
  return path.join(path.dirname(packageJsonPath), 'bin', nativeBinaryName());
}

function resolveDevBinary() {
  const repoRoot = path.resolve(__dirname, '..', '..', '..');
  const candidates = [
    path.join(repoRoot, 'src-tauri', 'target', 'cli-release', nativeBinaryName()),
    path.join(repoRoot, 'src-tauri', 'target', 'release', nativeBinaryName()),
    path.join(repoRoot, 'src-tauri', 'target', 'debug', nativeBinaryName())
  ];
  return candidates.find((candidate) => fs.existsSync(candidate));
}

function resolveBinary() {
  try {
    return resolveInstalledBinary();
  } catch (error) {
    const devBinary = resolveDevBinary();
    if (devBinary) return devBinary;

    const packageName = platformPackageName();
    throw new Error(
      `Could not find native Matpool binary for ${process.platform}-${process.arch}.\n` +
        `Expected optional package: ${packageName}\n` +
        `Original error: ${error.message}`
    );
  }
}

function updateSelf() {
  const npmCommand = 'npm';
  const result = spawnSync(npmCommand, ['install', '-g', '@mtswitch/switch@latest'], {
    env: process.env,
    shell: process.platform === 'win32',
    stdio: 'inherit'
  });

  if (result.error) {
    console.error(result.error.message);
    process.exit(1);
  }

  if (typeof result.status === 'number') {
    process.exit(result.status);
  }

  process.exit(result.signal ? 1 : 0);
}

if (process.argv[2] === 'update' || process.argv[2] === 'upgrade' || process.argv[2] === 'self-update') {
  updateSelf();
}

const binary = resolveBinary();
const result = spawnSync(binary, process.argv.slice(2), { stdio: 'inherit' });

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

if (typeof result.status === 'number') {
  process.exit(result.status);
}

process.exit(result.signal ? 1 : 0);
