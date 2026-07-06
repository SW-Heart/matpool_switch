#!/usr/bin/env node
import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

const packageByPlatform = {
  'darwin-arm64': 'switch-darwin-arm64',
  'darwin-x64': 'switch-darwin-x64',
  'linux-arm64': 'switch-linux-arm64',
  'linux-x64': 'switch-linux-x64',
  'win32-x64': 'switch-win32-x64',
}

const platformKey = `${process.platform}-${process.arch}`
const packageDirName = packageByPlatform[platformKey]

if (!packageDirName) {
  console.error(`Unsupported platform for npm binary staging: ${platformKey}`)
  process.exit(1)
}

const binaryName = process.platform === 'win32' ? 'matpool.exe' : 'matpool'
const profileFlagIndex = process.argv.indexOf('--profile')
const profile = process.argv.includes('--debug')
  ? 'debug'
  : profileFlagIndex >= 0 && process.argv[profileFlagIndex + 1]
    ? process.argv[profileFlagIndex + 1]
    : 'cli-release'
const source = path.join(repoRoot, 'src-tauri', 'target', profile, binaryName)
const targetDir = path.join(repoRoot, 'packages', packageDirName, 'bin')
const target = path.join(targetDir, binaryName)

if (!fs.existsSync(source)) {
  console.error(`Missing ${profile} binary: ${source}`)
  const buildProfile = profile === 'debug' ? '' : ` --profile ${profile}`
  console.error(`Build it first: cargo build --manifest-path src-tauri/Cargo.toml --bin matpool --no-default-features${buildProfile}`)
  process.exit(1)
}

fs.mkdirSync(targetDir, { recursive: true })
fs.copyFileSync(source, target)

if (process.platform !== 'win32') {
  fs.chmodSync(target, 0o755)
}

console.log(`Staged ${source} -> ${target}`)
