#!/usr/bin/env node
import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

const nativePackages = [
  {
    dir: 'switch-darwin-arm64',
    name: '@mtswitch/switch-darwin-arm64',
    os: 'darwin',
    cpu: 'arm64',
  },
  {
    dir: 'switch-darwin-x64',
    name: '@mtswitch/switch-darwin-x64',
    os: 'darwin',
    cpu: 'x64',
  },
  {
    dir: 'switch-linux-arm64',
    name: '@mtswitch/switch-linux-arm64',
    os: 'linux',
    cpu: 'arm64',
  },
  {
    dir: 'switch-linux-x64',
    name: '@mtswitch/switch-linux-x64',
    os: 'linux',
    cpu: 'x64',
  },
  {
    dir: 'switch-win32-x64',
    name: '@mtswitch/switch-win32-x64',
    os: 'win32',
    cpu: 'x64',
  },
]

const mainPackage = {
  dir: 'switch',
  name: '@mtswitch/switch',
}

const args = new Set(process.argv.slice(2))
const checkRegistry = args.has('--check-registry')
const allowExisting = args.has('--allow-existing')
const requireStagedBinaries = args.has('--require-staged-binaries')
const requireAllStagedBinaries = args.has('--all-staged-binaries')
const npmCommand = 'npm'
const failures = []

function readJson(relativePath) {
  const fullPath = path.join(repoRoot, relativePath)
  return JSON.parse(fs.readFileSync(fullPath, 'utf8'))
}

function assert(condition, message) {
  if (!condition) failures.push(message)
}

function packageJsonPath(packageDir) {
  return path.join('packages', packageDir, 'package.json')
}

function packageDir(packageDirName) {
  return path.join(repoRoot, 'packages', packageDirName)
}

function assertArrayEquals(actual, expected, label) {
  assert(Array.isArray(actual), `${label} must be an array`)
  if (!Array.isArray(actual)) return
  assert(
    actual.length === expected.length && expected.every((value, index) => actual[index] === value),
    `${label} must be ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`,
  )
}

function assertPackageFiles(pkg, expected, label) {
  assert(Array.isArray(pkg.files), `${label} files must be an array`)
  if (!Array.isArray(pkg.files)) return
  for (const file of expected) {
    assert(pkg.files.includes(file), `${label} files must include ${file}`)
  }
}

function npmViewPackageVersion(name, version) {
  const result = spawnSync(npmCommand, ['view', `${name}@${version}`, 'version', '--json'], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: process.env,
    shell: process.platform === 'win32',
  })

  if (result.status === 0) {
    if (allowExisting) {
      console.log(`${name}@${version} already exists on npm; release retries will skip it.`)
      return
    }
    failures.push(`${name}@${version} already exists on npm`)
    return
  }

  const output = `${result.stdout}\n${result.stderr}`
  if (!output.includes('E404') && !output.includes('404')) {
    failures.push(`npm registry check for ${name}@${version} failed:\n${output.trim()}`)
  }
}

const mainPkg = readJson(packageJsonPath(mainPackage.dir))
assert(mainPkg.name === mainPackage.name, `main package name must be ${mainPackage.name}`)
assert(mainPkg.bin?.matpool === 'bin/matpool.js', 'main package bin.matpool must be bin/matpool.js')
assert(mainPkg.engines?.node === '>=18', 'main package must require Node.js >=18')
assert(mainPkg.license === 'MIT', 'main package license must be MIT')
assert(Boolean(mainPkg.homepage), 'main package must define homepage')
assert(Boolean(mainPkg.repository?.url), 'main package must define repository.url')
assertPackageFiles(mainPkg, ['bin', 'README.md', 'package.json'], 'main package')

const mainReadmePath = path.join(packageDir(mainPackage.dir), 'README.md')
const mainShimPath = path.join(packageDir(mainPackage.dir), 'bin', 'matpool.js')
const npmPackageExistsScriptPath = path.join(repoRoot, 'scripts', 'check-npm-package-exists.js')
assert(fs.existsSync(mainReadmePath), 'main package README.md is missing')
assert(fs.existsSync(mainShimPath), 'main package bin/matpool.js is missing')
assert(fs.existsSync(npmPackageExistsScriptPath), 'scripts/check-npm-package-exists.js is missing')

const expectedOptionalDeps = Object.fromEntries(
  nativePackages.map((pkg) => [pkg.name, mainPkg.version]),
)
assert(
  JSON.stringify(mainPkg.optionalDependencies ?? {}) === JSON.stringify(expectedOptionalDeps),
  `main package optionalDependencies must exactly match native packages at ${mainPkg.version}`,
)

for (const nativePackage of nativePackages) {
  const pkg = readJson(packageJsonPath(nativePackage.dir))
  assert(pkg.name === nativePackage.name, `${nativePackage.dir} name must be ${nativePackage.name}`)
  assert(pkg.version === mainPkg.version, `${nativePackage.name} version must match ${mainPkg.version}`)
  assert(pkg.license === 'MIT', `${nativePackage.name} license must be MIT`)
  assert(Boolean(pkg.homepage), `${nativePackage.name} must define homepage`)
  assert(Boolean(pkg.repository?.url), `${nativePackage.name} must define repository.url`)
  assertArrayEquals(pkg.os, [nativePackage.os], `${nativePackage.name} os`)
  assertArrayEquals(pkg.cpu, [nativePackage.cpu], `${nativePackage.name} cpu`)
  assertPackageFiles(pkg, ['bin', 'package.json'], nativePackage.name)
  assert(
    pkg.files?.includes('!bin/.gitkeep'),
    `${nativePackage.name} files must exclude !bin/.gitkeep`,
  )

  const isCurrentPlatform =
    nativePackage.os === process.platform && nativePackage.cpu === process.arch
  if (requireStagedBinaries && (requireAllStagedBinaries || isCurrentPlatform)) {
    const binaryName = nativePackage.os === 'win32' ? 'matpool.exe' : 'matpool'
    const binaryPath = path.join(packageDir(nativePackage.dir), 'bin', binaryName)
    assert(fs.existsSync(binaryPath), `${nativePackage.name} staged binary is missing: ${binaryPath}`)
  }
}

const workflowPath = path.join(repoRoot, '.github', 'workflows', 'release-cli-npm.yml')
const workflow = fs.readFileSync(workflowPath, 'utf8')
for (const nativePackage of nativePackages) {
  assert(
    workflow.includes(`package: ${nativePackage.dir}`),
    `release workflow must include ${nativePackage.dir}`,
  )
}
assert(
  workflow.includes('--bin matpool --no-default-features --profile cli-release'),
  'release workflow must build the headless CLI with --no-default-features --profile cli-release',
)
assert(
  workflow.includes('publish-npm-package-if-needed.js'),
  'release workflow must publish npm packages through the idempotent publish script',
)
assert(
  workflow.includes('check-npm-package-exists.js'),
  'release workflow must skip native package builds when the package already exists',
)

if (checkRegistry) {
  npmViewPackageVersion(mainPkg.name, mainPkg.version)
  for (const nativePackage of nativePackages) {
    npmViewPackageVersion(nativePackage.name, mainPkg.version)
  }
}

if (failures.length > 0) {
  console.error('CLI npm release validation failed:')
  for (const failure of failures) {
    console.error(`- ${failure}`)
  }
  process.exit(1)
}

console.log(`CLI npm release validation passed for version ${mainPkg.version}.`)
