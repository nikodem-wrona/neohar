import { expect, test } from 'bun:test';
import type { ExecutionValue, WorkflowGraph, WorkflowNode } from '@neohar/transport';
import {
  addNode,
  connect,
  disconnect,
  edgePath,
  edges,
  fitView,
  insertAt,
  moveNode,
  needsYou,
  NODE_H,
  NODE_W,
  nodeAt,
  nodeLine,
  referenceChips,
  removeNode,
  runLine,
  toCanvas,
  uniqueId,
  zoomAt,
} from './model';

const node = (fields: Partial<WorkflowNode> & Pick<WorkflowNode, 'id'>): WorkflowNode => ({
  kind: 'agent',
  harness: 'claude',
  title: '',
  prompt: 'x',
  inputs: [],
  x: 0,
  y: 0,
  ...fields,
});

const review: WorkflowGraph = {
  name: 'Review',
  nodes: [
    node({ id: 'start', kind: 'manual', harness: null, title: 'Manual trigger', y: 140 }),
    node({ id: 'code', harness: 'codex', effort: 'high', inputs: ['start'], x: 300, y: 0 }),
    node({ id: 'logic', inputs: ['start'], x: 300, y: 140 }),
    node({ id: 'report', kind: 'report', inputs: ['code', 'logic'], x: 600, y: 140 }),
  ],
};

test('the zoom keeps the point under the pointer in its place and stops at the limits', () => {
  const view = { x: 10, y: 20, zoom: 1 };
  const at = { x: 200, y: 100 };
  const before = toCanvas(view, at);
  const zoomed = zoomAt(view, at, 2);
  expect(zoomed.zoom).toBe(2);
  expect(toCanvas(zoomed, at)).toEqual(before);
  expect(zoomAt(zoomed, at, 10).zoom).toBe(2);
  expect(zoomAt(view, at, 0.01).zoom).toBe(0.25);
});

test('fit shows every card in the middle of the area, never above 100 %', () => {
  const view = fitView(review.nodes, 2000, 1000);
  expect(view.zoom).toBe(1);
  const left = toCanvas(view, { x: 0, y: 0 });
  expect(left.x).toBeLessThan(0);
  const small = fitView(review.nodes, 400, 300);
  expect(small.zoom).toBeLessThan(1);
  expect(fitView([], 400, 300)).toEqual({ x: 40, y: 40, zoom: 1 });
});

test('the connections come from the inputs, and a curve goes from port to port', () => {
  expect(edges(review).map((e) => e.key)).toEqual([
    'start>code',
    'start>logic',
    'code>report',
    'logic>report',
  ]);
  expect(edgePath({ x: 0, y: 0 }, { x: 100, y: 50 })).toBe('M0 0C50 0 50 50 100 50');
});

test('a point finds the card under it', () => {
  expect(nodeAt(review.nodes, { x: 310, y: 10 })?.id).toBe('code');
  expect(nodeAt(review.nodes, { x: 300 + NODE_W + 1, y: 10 })).toBeUndefined();
  expect(nodeAt(review.nodes, { x: 5, y: 140 + NODE_H })?.id).toBe('start');
});

test('a new node gets a free id, a place on the grid and the connection of the drag', () => {
  expect(uniqueId(review, 'code')).toBe('code_2');
  expect(uniqueId(review, 'Security review')).toBe('security_review');
  expect(uniqueId(review, '42')).toBe('node');
  const added = addNode(
    review,
    { id: 'code', node: node({ id: '' }) },
    { x: 913, y: 307 },
    'logic',
  );
  expect(added.id).toBe('code_2');
  const made = added.graph.nodes.find((n) => n.id === 'code_2');
  expect(made?.inputs).toEqual(['logic']);
  expect(made?.x).toBe(920);
  expect(made?.y).toBe(260);
});

test('a connection needs two nodes, no loop to itself, and no input on the trigger', () => {
  expect(connect(review, 'code', 'code')).toBeNull();
  expect(connect(review, 'code', 'start')).toBeNull();
  expect(connect(review, 'start', 'code')).toBeNull();
  const chain = connect(review, 'code', 'logic');
  expect(chain?.nodes.find((n) => n.id === 'logic')?.inputs).toEqual(['start', 'code']);
  const back = disconnect(chain as WorkflowGraph, 'code', 'logic');
  expect(back.nodes.find((n) => n.id === 'logic')?.inputs).toEqual(['start']);
});

test('a removed node takes its connections with it, and the trigger stays', () => {
  const without = removeNode(review, 'code');
  expect(without.nodes.map((n) => n.id)).toEqual(['start', 'logic', 'report']);
  expect(without.nodes.find((n) => n.id === 'report')?.inputs).toEqual(['logic']);
  expect(removeNode(review, 'start')).toBe(review);
  expect(moveNode(review, 'logic', { x: 311, y: 149 }).nodes[2]).toMatchObject({ x: 320, y: 140 });
});

test('a card says its harness and settings, and a run says its state', () => {
  expect(nodeLine(review.nodes[1] as WorkflowNode)).toBe('Codex · high · read-only');
  expect(nodeLine(node({ id: 'fix', can_edit: true }))).toBe('Claude Code · can edit');
  expect(nodeLine(review.nodes[0] as WorkflowNode, 'origin/main')).toBe('base origin/main');
  expect(nodeLine(node({ id: 'r', kind: 'report', summary_by: 'claude' }))).toBe(
    'summary by Claude Code',
  );
  expect(runLine({ state: 'done', time_text: '6 min', verdict: 'warn' })).toBe(
    'done · 6 min · warn',
  );
  expect(runLine({ state: 'needs_you', time_text: '2 min', verdict: null })).toBe(
    'needs you · 2 min',
  );
});

test('the chips name the facts of the trigger and the reports of the inputs', () => {
  const chips = referenceChips(review, review.nodes[3] as WorkflowNode);
  expect(chips.map((c) => c.text)).toEqual([
    '{{start.branch}}',
    '{{start.base}}',
    '{{start.head}}',
    '{{start.folder}}',
    '{{start.notes}}',
    '{{code.report}}',
    '{{logic.report}}',
  ]);
  expect(insertAt('Read  now', 5, 5, '{{code.report}}')).toEqual({
    text: 'Read {{code.report}} now',
    at: 20,
  });
});

test('an execution needs the person when it pauses or a node waits for an answer', () => {
  const execution = (fields: Partial<ExecutionValue>): ExecutionValue => ({
    id: '01JAAAAAAAAAAAAAAAAAAAAAAA',
    workflow: 'review',
    label: 'Review 1',
    number: 1,
    folder: '/w',
    state: 'running',
    nodes: [],
    started_at: '2026-10-30T14:12:00Z',
    time_text: '',
    dir: '/d',
    ...fields,
  });
  expect(needsYou(execution({}))).toBe(false);
  expect(needsYou(execution({ state: 'paused' }))).toBe(true);
  expect(needsYou(execution({ nodes: [{ node: 'code', state: 'needs_you' }] }))).toBe(true);
});
