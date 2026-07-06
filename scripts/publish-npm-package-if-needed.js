#!/usr/bin/env node
import fs from 'node:fs'
import path from 'node:path'
import { spawnSync } from 'node:child_process'

const packageJsonPath = path.join(process.cwd(), 'package.json')
const pkg = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'))
const spec = `${pkg.name}@${pkg.version}`
const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm'

function run(command, args, options = {}) {
  return spawnSync(command, args, {
    cwd: process.cwd(),
    encoding: 'utf8',
    stdio: options.stdio ?? 'pipe',
    env: process.env,
  })
}

const view = run(npmCommand, ['view', spec, 'version', '--json'])
if (view.status === 0) {
  console.log(`${spec} already exists on npm; skipping publish.`)
  process.exit(0)
}

const viewOutput = `${view.stdout}\n${view.stderr}`
if (!viewOutput.includes('E404') && !viewOutput.includes('404')) {
  process.stderr.write(viewOutput)
  process.exit(view.status ?? 1)
}

console.log(`Publishing ${spec}...`)
const publish = run(npmCommand, ['publish', '--access', 'public'], { stdio: 'inherit' })
if (publish.error) {
  console.error(publish.error.message)
  process.exit(1)
}
process.exit(typeof publish.status === 'number' ? publish.status : publish.signal ? 1 : 0)
