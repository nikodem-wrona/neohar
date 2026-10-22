// Writes `packages/transport/src/generated.ts` from the contract of the core.
//
//   bun tools/gen-ts/index.ts            run `nh api schema` and write the file
//   bun tools/gen-ts/index.ts --check    fail if the file on disk is not what the schema makes
//   bun tools/gen-ts/index.ts --schema FILE   read the schema from a file instead
//
// `mise run gen:api` runs it. CI runs it again and compares the file with git.

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { format, resolveConfig } from 'prettier';

import { type ApiDocument, emit } from './src/emit';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const target = join(root, 'packages/transport/src/generated.ts');

function schemaText(file: string | undefined): string {
  if (file !== undefined) return readFileSync(file, 'utf8');
  const run = Bun.spawnSync(['cargo', 'run', '-q', '--locked', '-p', 'nh', '--', 'api', 'schema'], {
    cwd: root,
    stderr: 'inherit',
  });
  if (run.exitCode !== 0) throw new Error(`nh api schema failed with code ${run.exitCode}`);
  return run.stdout.toString();
}

async function generate(file: string | undefined): Promise<string> {
  const doc = JSON.parse(schemaText(file)) as ApiDocument;
  const config = (await resolveConfig(target)) ?? {};
  return format(emit(doc), { ...config, parser: 'typescript' });
}

const args = process.argv.slice(2);
const check = args.includes('--check');
const schemaIndex = args.indexOf('--schema');
const schemaFile = schemaIndex >= 0 ? args[schemaIndex + 1] : undefined;

const text = await generate(schemaFile);
if (check) {
  const current = readFileSync(target, 'utf8');
  if (current !== text) {
    console.error('packages/transport/src/generated.ts is out of date. Run `mise run gen:api`.');
    process.exit(1);
  }
} else {
  writeFileSync(target, text);
  console.log(`wrote ${target}`);
}
