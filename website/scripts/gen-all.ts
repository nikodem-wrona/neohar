// Writes every generated page of the site. Run it with `bun run gen` from `website/`.
//
// The CLI goes first: the API pages link each method to its command, from the list that the CLI
// generator saves in `scripts/data/cli.json`.
//
// With `--check` (`mise run check:docs`), it writes nothing and exits with 1 when a generated file
// is not what a run without `--check` writes.
import { generateApi } from './gen-api';
import { generateCli } from './gen-cli';
import { generateConfig } from './gen-config';
import { generateTasks } from './gen-tasks';
import { checkFaults, checkOnly } from './lib';

generateCli();
generateApi();
generateConfig();
generateTasks();

if (checkOnly) {
  const faults = checkFaults();
  if (faults.length) {
    console.error('These generated files are not what `bun run gen` writes:');
    for (const fault of faults) console.error(`  ${fault}`);
    console.error('Run `bun run gen` in website/ and commit the pages.');
    process.exit(1);
  }
  console.log('Every generated file is up to date.');
}
