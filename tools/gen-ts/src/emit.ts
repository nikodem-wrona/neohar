// Turns the schema document of `nh api schema` into TypeScript.
//
// The document is JSON Schema (draft 2020-12) made by `schemars`, plus the lists of methods,
// notifications, projections, events and streams. The output has the types of every definition
// that the contract reaches, and the maps from a name to its types: `MethodMap`,
// `ProjectionValueMap`, `EventPayloadMap`, `NotificationMap` and `StreamParamsMap`.
//
// Integers of 64 bits are `number`. The core keeps them below 2^53 (a counter, a line number, a
// time in milliseconds), and JSON carries nothing else.

export type Schema = boolean | SchemaObject;

export interface SchemaObject {
  $ref?: string;
  type?: string | string[];
  const?: unknown;
  enum?: unknown[];
  oneOf?: Schema[];
  anyOf?: Schema[];
  allOf?: Schema[];
  properties?: Record<string, Schema>;
  required?: string[];
  items?: Schema;
  prefixItems?: Schema[];
  additionalProperties?: Schema;
  description?: string;
}

export interface MethodDoc {
  name: string;
  scopes: string[];
  params: Schema;
  result: Schema;
}

export interface NotificationDoc {
  name: string;
  params: Schema;
}

export interface ProjectionDoc {
  name: string;
  key: string;
  singleton: boolean;
  parameter: string | null;
  value: Schema;
}

export interface EventDoc {
  kind: string;
  v: number;
  payload: Schema;
}

export interface StreamDoc {
  kind: string;
  direction: string;
  params: Schema;
}

export interface ApiDocument {
  api_version: string;
  protocols: { frame: number; holder: number };
  error_codes: { code: string; rpc_code: number }[];
  methods: MethodDoc[];
  notifications: NotificationDoc[];
  projections: ProjectionDoc[];
  events: EventDoc[];
  streams: StreamDoc[];
  $defs: Record<string, Schema>;
}

const REF_PREFIX = '#/$defs/';

/** The name a `$ref` points to. */
function refName(ref: string): string {
  if (!ref.startsWith(REF_PREFIX)) throw new Error(`unsupported $ref: ${ref}`);
  return ref.slice(REF_PREFIX.length);
}

/** A string as a TypeScript literal with single quotes. */
function literal(value: unknown): string {
  if (typeof value === 'string') {
    return `'${value.replace(/\\/g, '\\\\').replace(/'/g, "\\'").replace(/\n/g, '\\n')}'`;
  }
  if (value === null) return 'null';
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  throw new Error(`unsupported literal: ${JSON.stringify(value)}`);
}

/** The key of a property: a bare name if it is one, else quoted. */
function key(name: string): string {
  return /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(name) ? name : literal(name);
}

/** Wraps a union or an intersection in parentheses when it goes into an array or an intersection. */
function paren(type: string): string {
  return /[|&]/.test(type) && !/^[[{(]/.test(type) ? `(${type})` : type;
}

function docComment(description: string | undefined, indent: string): string {
  if (!description) return '';
  const lines = description.replace(/\*\//g, '*\\/').split('\n');
  if (lines.length === 1) return `${indent}/** ${lines[0]} */\n`;
  return `${indent}/**\n${lines.map((line) => `${indent} * ${line}`.trimEnd()).join('\n')}\n${indent} */\n`;
}

/** The keywords that make an object type next to a `$ref` or a union. */
function hasObjectShape(schema: SchemaObject): boolean {
  return schema.properties !== undefined || schema.additionalProperties !== undefined;
}

/** The TypeScript type of a schema, as an expression. */
export function typeOf(schema: Schema, indent = ''): string {
  if (schema === true) return 'unknown';
  if (schema === false) return 'never';
  const parts: string[] = [];

  if (schema.$ref !== undefined) parts.push(refName(schema.$ref));
  if (schema.const !== undefined) return literal(schema.const);
  if (schema.enum !== undefined) return schema.enum.map(literal).join(' | ');

  const unions = schema.oneOf ?? schema.anyOf;
  if (unions !== undefined) {
    parts.push(unions.map((member) => paren(typeOf(member, indent))).join(' | '));
  }
  if (schema.allOf !== undefined) {
    parts.push(schema.allOf.map((member) => paren(typeOf(member, indent))).join(' & '));
  }

  const types =
    schema.type === undefined ? [] : Array.isArray(schema.type) ? schema.type : [schema.type];
  const base: string[] = [];
  for (const type of types) {
    switch (type) {
      case 'string':
        base.push('string');
        break;
      case 'integer':
      case 'number':
        base.push('number');
        break;
      case 'boolean':
        base.push('boolean');
        break;
      case 'null':
        base.push('null');
        break;
      case 'array':
        base.push(arrayType(schema, indent));
        break;
      case 'object':
        if (parts.length === 0 || hasObjectShape(schema)) base.push(objectType(schema, indent));
        break;
      default:
        throw new Error(`unsupported type: ${type}`);
    }
  }
  if (types.length === 0 && hasObjectShape(schema)) base.push(objectType(schema, indent));
  if (base.length > 0) parts.push(base.join(' | '));

  if (parts.length === 0) return 'unknown';
  if (parts.length === 1) return parts[0] ?? 'unknown';
  return parts.map(paren).join(' & ');
}

function arrayType(schema: SchemaObject, indent: string): string {
  if (schema.prefixItems !== undefined) {
    return `[${schema.prefixItems.map((item) => typeOf(item, indent)).join(', ')}]`;
  }
  if (schema.items === undefined) return 'unknown[]';
  return `${paren(typeOf(schema.items, indent))}[]`;
}

function objectType(schema: SchemaObject, indent: string): string {
  const properties = schema.properties ?? {};
  const required = new Set(schema.required ?? []);
  const names = Object.keys(properties);
  const extra = schema.additionalProperties;
  const record =
    extra !== undefined && extra !== false
      ? `Record<string, ${extra === true ? 'unknown' : typeOf(extra, indent)}>`
      : undefined;
  if (names.length === 0) return record ?? 'Record<string, never>';
  const inner = `${indent}  `;
  const fields = names.map((name) => {
    const property = properties[name];
    if (property === undefined) throw new Error(`no schema for ${name}`);
    const optional = required.has(name) ? '' : '?';
    return `${docComment(typeof property === 'object' ? property.description : undefined, inner)}${inner}${key(name)}${optional}: ${typeOf(property, inner)};`;
  });
  const body = `{\n${fields.join('\n')}\n${indent}}`;
  return record === undefined ? body : `${body} & ${record}`;
}

/** The definitions that a schema refers to, directly. */
function refsOf(schema: Schema, into: Set<string>): void {
  if (typeof schema !== 'object') return;
  if (schema.$ref !== undefined) into.add(refName(schema.$ref));
  for (const member of [
    ...(schema.oneOf ?? []),
    ...(schema.anyOf ?? []),
    ...(schema.allOf ?? []),
  ]) {
    refsOf(member, into);
  }
  for (const property of Object.values(schema.properties ?? {})) refsOf(property, into);
  for (const item of schema.prefixItems ?? []) refsOf(item, into);
  if (schema.items !== undefined) refsOf(schema.items, into);
  if (schema.additionalProperties !== undefined) refsOf(schema.additionalProperties, into);
}

/** Every definition that the contract reaches. */
export function reachable(doc: ApiDocument, roots: Schema[]): string[] {
  const seen = new Set<string>();
  const queue: Schema[] = [...roots];
  while (queue.length > 0) {
    const schema = queue.pop();
    if (schema === undefined) break;
    const found = new Set<string>();
    refsOf(schema, found);
    for (const name of found) {
      if (seen.has(name)) continue;
      seen.add(name);
      const definition = doc.$defs[name];
      if (definition === undefined) throw new Error(`the definition ${name} is missing`);
      queue.push(definition);
    }
  }
  return [...seen].sort();
}

/** `export interface` for an object with properties only, else `export type`. */
function declaration(name: string, schema: Schema): string {
  const description = typeof schema === 'object' ? schema.description : undefined;
  const comment = docComment(description, '');
  const plainObject =
    typeof schema === 'object' &&
    schema.type === 'object' &&
    schema.properties !== undefined &&
    schema.oneOf === undefined &&
    schema.anyOf === undefined &&
    schema.allOf === undefined &&
    schema.$ref === undefined &&
    schema.additionalProperties === undefined &&
    Object.keys(schema.properties).length > 0;
  if (plainObject) {
    return `${comment}export interface ${name} ${typeOf(schema)}\n`;
  }
  return `${comment}export type ${name} = ${typeOf(schema)};\n`;
}

/** A property key of a map type: `'pane.split'`. */
function mapEntry(name: string, body: string): string {
  return `  ${literal(name)}: ${body};\n`;
}

function union(names: string[]): string {
  return names.length === 0 ? 'never' : names.map(literal).join(' | ');
}

/** The whole `generated.ts`. */
export function emit(doc: ApiDocument): string {
  const roots: Schema[] = [
    ...doc.methods.flatMap((method) => [method.params, method.result]),
    ...doc.notifications.map((notification) => notification.params),
    ...doc.projections.map((projection) => projection.value),
    ...doc.events.map((event) => event.payload),
    ...doc.streams.map((stream) => stream.params),
    { $ref: `${REF_PREFIX}CallError` },
  ];
  const names = reachable(doc, roots);

  let out = `// GENERATED by tools/gen-ts from \`nh api schema\`. Do not edit.\n`;
  out += `// Regenerate with \`mise run gen:api\`. CI fails when this file is out of date.\n\n`;
  out += `/** The version of the API. */\n`;
  out += `export const API_VERSION = ${literal(doc.api_version)};\n\n`;
  out += `/** The versions of the binary formats. */\n`;
  out += `export const PROTOCOLS = { frame: ${doc.protocols.frame}, holder: ${doc.protocols.holder} } as const;\n\n`;

  out += `// --- Types -------------------------------------------------------------------------\n\n`;
  for (const name of names) {
    const definition = doc.$defs[name];
    if (definition === undefined) throw new Error(`the definition ${name} is missing`);
    out += `${declaration(name, definition)}\n`;
  }

  out += `// --- Errors ------------------------------------------------------------------------\n\n`;
  out += `/** Every error code, with the integer of JSON-RPC. */\n`;
  out += `export const ERROR_CODES = {\n`;
  for (const { code, rpc_code } of doc.error_codes) out += `  ${key(code)}: ${rpc_code},\n`;
  out += `} as const;\n\n`;

  out += `// --- Methods -----------------------------------------------------------------------\n\n`;
  out += `/** Who may call a method. */\nexport type Scope = 'ui' | 'cli' | 'pane' | 'dev' | 'phone';\n\n`;
  out += `/** Every method with its params and result. */\nexport interface MethodMap {\n`;
  for (const method of doc.methods) {
    out += mapEntry(
      method.name,
      `{ params: ${typeOf(method.params, '  ')}; result: ${typeOf(method.result, '  ')} }`,
    );
  }
  out += `}\n\nexport type Method = keyof MethodMap;\n`;
  out += `export type Params<M extends Method> = MethodMap[M]['params'];\n`;
  out += `export type Result<M extends Method> = MethodMap[M]['result'];\n\n`;
  for (const scope of ['ui', 'cli', 'pane', 'dev']) {
    const members = doc.methods
      .filter((method) => method.scopes.includes(scope))
      .map((m) => m.name);
    const type = `${scope[0]?.toUpperCase() ?? ''}${scope.slice(1)}Method`;
    out += `/** The methods that the \`${scope}\` scope may call. */\nexport type ${type} = ${union(members)};\n\n`;
  }
  out += `/** The scopes of every method. */\nexport const METHOD_SCOPES: { readonly [M in Method]: readonly Scope[] } = {\n`;
  for (const method of doc.methods) {
    out += `  ${literal(method.name)}: [${method.scopes.map(literal).join(', ')}],\n`;
  }
  out += `};\n\n`;

  out += `/** The notifications of the server, with their params. */\nexport interface NotificationMap {\n`;
  for (const notification of doc.notifications) {
    out += mapEntry(notification.name, typeOf(notification.params, '  '));
  }
  out += `}\n\n`;

  out += `// --- Projections -------------------------------------------------------------------\n\n`;
  out += `/** The value type of each projection. The key is a string. */\nexport interface ProjectionValueMap {\n`;
  for (const projection of doc.projections) {
    out += `  ${key(projection.name)}: ${typeOf(projection.value, '  ')};\n`;
  }
  out += `}\n\nexport type ProjectionBase = keyof ProjectionValueMap;\n\n`;
  const parameterized = doc.projections.filter((p) => p.parameter !== null).map((p) => p.name);
  const plain = doc.projections.filter((p) => p.parameter === null).map((p) => p.name);
  out += `/** The name of a projection, with its parameter where it has one. */\n`;
  out += `export type ProjectionName =\n  | ${union(plain)}\n`;
  for (const name of parameterized) out += `  | \`${name}:\${string}\`\n`;
  out = out.replace(/\n$/, ';\n\n');
  const singletons = doc.projections.filter((p) => p.singleton).map((p) => p.name);
  out += `/** The projections with one key, \`_\`. */\n`;
  out += `export const SINGLETON_PROJECTIONS = [${singletons.map(literal).join(', ')}] as const;\n`;
  out += `export const SINGLETON_KEY = '_';\n\n`;

  out += `// --- Events ------------------------------------------------------------------------\n\n`;
  out += `/** The payload of each kind of event. */\nexport interface EventPayloadMap {\n`;
  for (const event of doc.events) out += mapEntry(event.kind, typeOf(event.payload, '  '));
  out += `}\n\nexport type EventKind = keyof EventPayloadMap;\n\n`;

  out += `// --- Streams -----------------------------------------------------------------------\n\n`;
  out += `/** The params that open each stream. The bodies are binary. */\nexport interface StreamParamsMap {\n`;
  for (const stream of doc.streams)
    out += `  ${key(stream.kind)}: ${typeOf(stream.params, '  ')};\n`;
  out += `}\n`;
  return out;
}
