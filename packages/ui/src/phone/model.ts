// What the phone app needs to choose what it shows: the route of the address and the code
// of a pairing. The core decides the state of each run and the order of every list.

/**
 * What the hash of the address asks the desk on the phone to show, in place of the
 * `neohar://` links of the desk. A push opens `#/run/<id>`, and the push of another machine
 * `#/machine/<label>`.
 */
export type PhoneRoute =
  | { kind: 'desk' }
  | { kind: 'machine'; machine: string }
  | { kind: 'pane'; pane: string }
  | { kind: 'interaction'; interaction: string }
  | { kind: 'run'; run: string }
  | { kind: 'settings'; section: string | null };

/** An id of the core: a ULID. */
const ID = /^[0-9A-HJKMNP-TV-Z]{26}$/;

/** A name of a section of the settings. */
const SECTION = /^[a-z][a-z0-9_]*$/;

/** The label of a machine of `config.toml`, or `local` for the Mac itself. */
const MACHINE = /^[a-z0-9-]{1,32}$/;

/** The route of a hash. An address that the desk does not know shows the desk. */
export function routeOf(hash: string): PhoneRoute {
  const parts = hash.replace(/^#\/?/, '').split('/');
  const [kind, id, ...rest] = parts;
  if (rest.length > 0) return { kind: 'desk' };
  if (kind === 'settings') {
    if (id === undefined || id === '') return { kind: 'settings', section: null };
    return SECTION.test(id) ? { kind: 'settings', section: id } : { kind: 'desk' };
  }
  if (kind === 'machine') {
    return id !== undefined && MACHINE.test(id)
      ? { kind: 'machine', machine: id }
      : { kind: 'desk' };
  }
  if (id === undefined || !ID.test(id)) return { kind: 'desk' };
  switch (kind) {
    case 'pane':
      return { kind: 'pane', pane: id };
    case 'interaction':
      return { kind: 'interaction', interaction: id };
    case 'run':
      return { kind: 'run', run: id };
    default:
      return { kind: 'desk' };
  }
}

/** The hash of a route. */
export function hashOf(route: PhoneRoute): string {
  switch (route.kind) {
    case 'desk':
      return '#/';
    case 'machine':
      return `#/machine/${route.machine}`;
    case 'pane':
      return `#/pane/${route.pane}`;
    case 'interaction':
      return `#/interaction/${route.interaction}`;
    case 'run':
      return `#/run/${route.run}`;
    case 'settings':
      return route.section ? `#/settings/${route.section}` : '#/settings';
  }
}

/** The code of a pairing in the address of the page: `?code=<code>`. */
export function pairCodeOf(search: string): string | null {
  const code = new URLSearchParams(search).get('code');
  return code && code.trim() !== '' ? code.trim() : null;
}
