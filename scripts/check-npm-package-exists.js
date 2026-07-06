#!/usr/bin/env node
import fs from 'node:fs'
import path from 'node:path'

const packageJsonPath = path.join(process.cwd(), 'package.json')
const pkg = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'))
const spec = `${pkg.name}@${pkg.version}`
const registryPackageUrl = `https://registry.npmjs.org/${encodeURIComponent(pkg.name)}`

function setOutput(name, value) {
  if (process.env.GITHUB_OUTPUT) {
    fs.appendFileSync(process.env.GITHUB_OUTPUT, `${name}=${value}\n`)
  }
}

const response = await fetch(registryPackageUrl, {
  headers: {
    accept: 'application/vnd.npm.install-v1+json',
  },
})

if (response.status === 404) {
  console.log(`${spec} does not exist on npm.`)
  setOutput('exists', 'false')
  process.exit(0)
}

if (!response.ok) {
  console.error(`Failed to check ${spec}: HTTP ${response.status}`)
  process.exit(1)
}

const metadata = await response.json()
const exists = Boolean(metadata.versions?.[pkg.version])
console.log(`${spec} ${exists ? 'already exists' : 'does not exist'} on npm.`)
setOutput('exists', exists ? 'true' : 'false')
