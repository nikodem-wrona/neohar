import { describe, expect, test } from 'bun:test';

import { type ApiDocument, type Schema, emit, reachable, typeOf } from './emit';

describe('typeOf', () => {
  test('scalars', () => {
    expect(typeOf({ type: 'string' })).toBe('string');
    expect(typeOf({ type: 'integer', format: 'uint64' } as Schema)).toBe('number');
    expect(typeOf({ type: 'number' })).toBe('number');
    expect(typeOf({ type: 'boolean' })).toBe('boolean');
    expect(typeOf(true)).toBe('unknown');
    expect(typeOf({})).toBe('unknown');
  });

  test('a nullable type is a union with null', () => {
    expect(typeOf({ type: ['string', 'null'] })).toBe('string | null');
  });

  test('constants and enums are literals', () => {
    expect(typeOf({ const: 'pane', type: 'string' })).toBe("'pane'");
    expect(typeOf({ type: 'string', enum: ['a', "b'c"] })).toBe("'a' | 'b\\'c'");
  });

  test('arrays, tuples and maps', () => {
    expect(typeOf({ type: 'array', items: { type: 'string' } })).toBe('string[]');
    expect(
      typeOf({ type: 'array', items: { anyOf: [{ type: 'string' }, { type: 'number' }] } }),
    ).toBe('(string | number)[]');
    expect(typeOf({ type: 'array', prefixItems: [{ type: 'integer' }, { type: 'string' }] })).toBe(
      '[number, string]',
    );
    expect(typeOf({ type: 'object', additionalProperties: { type: 'string' } })).toBe(
      'Record<string, string>',
    );
    expect(typeOf({ type: 'object' })).toBe('Record<string, never>');
  });

  test('an object marks the optional properties', () => {
    const text = typeOf({
      type: 'object',
      properties: { a: { type: 'string' }, b: { type: ['integer', 'null'] } },
      required: ['a'],
    });
    expect(text).toContain('a: string;');
    expect(text).toContain('b?: number | null;');
  });

  test('a reference with extra properties is an intersection', () => {
    const text = typeOf({
      $ref: '#/$defs/ApprovalDecision',
      type: 'object',
      properties: { kind: { const: 'approval', type: 'string' } },
      required: ['kind'],
    });
    expect(text).toStartWith('ApprovalDecision & {');
    expect(text).toContain("kind: 'approval';");
  });

  test('oneOf is a union and allOf is an intersection', () => {
    expect(typeOf({ oneOf: [{ $ref: '#/$defs/A' }, { $ref: '#/$defs/B' }] })).toBe('A | B');
    expect(typeOf({ allOf: [{ $ref: '#/$defs/A' }, { $ref: '#/$defs/B' }] })).toBe('A & B');
  });

  test('an object next to anyOf (a flattened enum) is an intersection', () => {
    const text = typeOf({
      type: 'object',
      properties: { pane: { $ref: '#/$defs/RefPaneId' } },
      required: ['pane'],
      anyOf: [
        { type: 'object', properties: { top_line: { type: 'integer' } }, required: ['top_line'] },
        { type: 'object', properties: { delta: { type: 'integer' } }, required: ['delta'] },
      ],
    });
    expect(text).toContain('pane: RefPaneId;');
    expect(text).toContain('|');
    expect(text).toContain('&');
  });

  test('a reference outside the definitions is refused', () => {
    expect(() => typeOf({ $ref: 'http://example.com/x' })).toThrow();
  });
});

const doc: ApiDocument = {
  api_version: 'neohar/1',
  protocols: { frame: 1, holder: 2 },
  error_codes: [
    { code: 'not_found', rpc_code: -32000 },
    { code: 'internal', rpc_code: -32603 },
  ],
  methods: [
    {
      name: 'pane.close',
      scopes: ['ui', 'cli'],
      params: { $ref: '#/$defs/PaneClose' },
      result: { $ref: '#/$defs/Empty' },
    },
    {
      name: 'adapter.signal',
      scopes: ['pane'],
      params: { $ref: '#/$defs/Empty' },
      result: { $ref: '#/$defs/Empty' },
    },
  ],
  notifications: [{ name: 'events.truncated', params: { $ref: '#/$defs/Empty' } }],
  projections: [
    {
      name: 'tree',
      key: 'workspace id',
      singleton: false,
      parameter: null,
      value: { type: 'string' },
    },
    {
      name: 'timeline',
      key: 'item id',
      singleton: false,
      parameter: 'run id',
      value: { type: 'string' },
    },
    { name: 'health', key: '_', singleton: true, parameter: null, value: { type: 'string' } },
  ],
  events: [{ kind: 'pane.exited', v: 1, payload: { $ref: '#/$defs/Empty' } }],
  streams: [{ kind: 'render', direction: 'core_to_ui', params: { $ref: '#/$defs/Empty' } }],
  $defs: {
    PaneClose: {
      type: 'object',
      description: 'Closes a pane.',
      properties: { pane: { $ref: '#/$defs/PaneId' }, force: { type: 'boolean' } },
      required: ['pane'],
    },
    PaneId: { type: 'string', description: 'A pane.' },
    Empty: { type: 'object' },
    CallError: {
      type: 'object',
      properties: { message: { type: 'string' } },
      required: ['message'],
    },
    Unused: { type: 'string' },
  },
};

describe('reachable', () => {
  test('follows references and leaves out what nothing uses', () => {
    const names = reachable(doc, [{ $ref: '#/$defs/PaneClose' }]);
    expect(names).toEqual(['PaneClose', 'PaneId']);
  });

  test('a missing definition is an error', () => {
    expect(() => reachable(doc, [{ $ref: '#/$defs/Nothing' }])).toThrow();
  });
});

describe('emit', () => {
  const text = emit(doc);

  test('writes interfaces, with the comment of the schema', () => {
    expect(text).toContain('/** Closes a pane. */\nexport interface PaneClose {');
    expect(text).toContain('export type PaneId = string;');
    expect(text).not.toContain('Unused');
  });

  test('writes the method map and the types that use it', () => {
    expect(text).toContain("'pane.close': { params: PaneClose; result: Empty }");
    expect(text).toContain('export type Method = keyof MethodMap;');
    expect(text).toContain("export type UiMethod = 'pane.close';");
    expect(text).toContain("export type PaneMethod = 'adapter.signal';");
    expect(text).toContain('export type DevMethod = never;');
  });

  test('writes the projections, the events, the streams and the error codes', () => {
    expect(text).toContain('| `timeline:${string}`;');
    expect(text).toContain("SINGLETON_PROJECTIONS = ['health'] as const");
    expect(text).toContain("'pane.exited': Empty;");
    expect(text).toContain('render: Empty;');
    expect(text).toContain('not_found: -32000,');
    expect(text).toContain('internal: -32603,');
  });

  test('is the same twice', () => {
    expect(emit(doc)).toBe(text);
  });
});
