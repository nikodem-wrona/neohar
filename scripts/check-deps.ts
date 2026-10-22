// `mise run check:deps`: the crate dependency rules and the wasm-bindgen pin.
//
// RULES is the table of the crate dependency rules. The change that creates a crate adds its row.
//   allow:  the workspace crates the crate may depend on directly ('any' = no limit).
//   forbid: crates that must not appear anywhere in its normal dependency tree.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

type Rule = { allow: string[] | 'any'; forbid: string[] };

export const RULES: Record<string, Rule> = {
  'nh-proto': { allow: [], forbid: ['tokio'] },
  'nh-hold': { allow: ['nh-proto'], forbid: ['tokio', 'tracing', 'serde_json'] },
  'nh-term': { allow: ['nh-proto'], forbid: ['tokio'] },
  'nh-store': { allow: ['nh-proto', 'nh-redact'], forbid: ['tokio'] },
  'nh-rich': { allow: [], forbid: ['tokio'] },
  'nh-fixture': { allow: [], forbid: ['tokio'] },
  'nh-record': { allow: ['nh-proto'], forbid: ['tokio'] },
  'mock-llm': {
    allow: [],
    forbid: ['nh-core', 'nh-proto', 'nh-term', 'nh-hold', 'nh-store', 'tauri'],
  },
  'nh-golden': {
    allow: ['mock-llm', 'nh-proto', 'nh-term', 'nh-record', 'nh-core'],
    forbid: ['nh-hold', 'tauri'],
  },
  'nh-chaos': {
    allow: ['nh-proto', 'nh-term', 'nh-store', 'nh-core', 'nh-fixture'],
    forbid: ['tauri'],
  },
  'nh-snapcheck': { allow: ['nh-proto', 'nh-hold', 'nh-term', 'nh-fixture'], forbid: ['tokio'] },
  'nh-bench': { allow: ['nh-proto', 'nh-term'], forbid: ['tokio'] },
  'nh-keytest': { allow: ['nh-proto', 'nh-term'], forbid: ['tokio'] },
  'proto-wasm': { allow: ['nh-proto'], forbid: ['tokio'] },
  'nh-redact': { allow: [], forbid: ['tokio'] },
  'nh-agents': {
    allow: ['nh-proto', 'nh-term', 'nh-rich', 'nh-redact'],
    forbid: ['nh-core', 'tauri'],
  },
  'nh-tasks': { allow: ['nh-proto'], forbid: ['nh-core', 'tauri'] },
  'nh-core': {
    allow: [
      'nh-proto',
      'nh-hold',
      'nh-term',
      'nh-store',
      'nh-rich',
      'nh-redact',
      'nh-agents',
      'nh-tasks',
    ],
    forbid: ['tauri'],
  },
  'nh-remote': { allow: ['nh-core', 'nh-proto'], forbid: ['tauri'] },
  'nh-web': { allow: ['nh-core', 'nh-proto'], forbid: ['tauri'] },
  nh: { allow: ['nh-proto', 'nh-hold', 'nh-core', 'nh-remote', 'nh-web'], forbid: ['tauri'] },
  'neohar-desktop': { allow: ['nh-core', 'nh-proto', 'nh-remote', 'nh-web'], forbid: [] },
};

/** The crate names in the output of `cargo tree --prefix none`. */
export function treeNames(output: string): Set<string> {
  const names = new Set<string>();
  for (const line of output.split('\n')) {
    const name = line.trim().split(' ')[0];
    if (name) names.add(name);
  }
  return names;
}

/** The problems of one crate, given its direct workspace dependencies and its whole tree. */
export function problems(
  crate: string,
  rule: Rule | undefined,
  direct: string[],
  tree: Set<string>,
): string[] {
  if (!rule) {
    return [`${crate}: no row in RULES (scripts/check-deps.ts)`];
  }
  const out: string[] = [];
  if (rule.allow !== 'any') {
    for (const dep of direct.filter((d) => !rule.allow.includes(d))) {
      out.push(`${crate}: may not depend on ${dep}`);
    }
  }
  for (const name of rule.forbid.filter((f) => tree.has(f))) {
    out.push(`${crate}: ${name} is in its dependency tree`);
  }
  return out;
}

/** The problem with the wasm-bindgen pin, or null. The CLI must equal the crate in Cargo.lock. */
export function pinProblem(lock: string, mise: string): string | null {
  const crate = (
    Bun.TOML.parse(lock) as { package?: { name: string; version: string }[] }
  ).package?.find((p) => p.name === 'wasm-bindgen');
  if (!crate) return null;
  const tools = (Bun.TOML.parse(mise) as { tools?: Record<string, string> }).tools ?? {};
  const cli = tools['cargo:wasm-bindgen-cli'];
  return cli === crate.version
    ? null
    : `mise.toml pins cargo:wasm-bindgen-cli ${cli ?? '(missing)'} but Cargo.lock has wasm-bindgen ${crate.version}`;
}

function run(cmd: string[], cwd: string): string {
  const result = Bun.spawnSync(cmd, { cwd, stderr: 'pipe' });
  if (result.exitCode !== 0) {
    throw new Error(`${cmd.join(' ')} failed:\n${result.stderr.toString()}`);
  }
  return result.stdout.toString();
}

if (import.meta.main) {
  const root = join(import.meta.dir, '..');
  type Package = {
    id: string;
    name: string;
    dependencies: { name: string; kind: string | null; path?: string }[];
  };
  const metadata = JSON.parse(
    run(['cargo', 'metadata', '--format-version', '1', '--locked'], root),
  ) as {
    packages: Package[];
    workspace_members: string[];
  };
  const members = metadata.packages.filter((p) => metadata.workspace_members.includes(p.id));
  const memberNames = new Set(members.map((m) => m.name));

  const found: string[] = [];
  for (const member of members) {
    const direct = member.dependencies
      .filter((d) => d.kind === null && memberNames.has(d.name))
      .map((d) => d.name);
    const tree = treeNames(
      run(
        [
          'cargo',
          'tree',
          '-p',
          member.name,
          '-e',
          'normal',
          '--prefix',
          'none',
          '--target',
          'all',
          '--locked',
        ],
        root,
      ),
    );
    found.push(...problems(member.name, RULES[member.name], direct, tree));
  }
  const pin = pinProblem(
    readFileSync(join(root, 'Cargo.lock'), 'utf8'),
    readFileSync(join(root, 'mise.toml'), 'utf8'),
  );
  if (pin) found.push(pin);

  if (found.length > 0) {
    console.error(found.join('\n'));
    process.exit(1);
  }
  console.log(`check:deps: ${members.length} crates follow the dependency rules`);
}
