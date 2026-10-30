// What the Workflows view needs from `workflows`, `workflow_executions` and the graph that
// `workflow.show` gives. The core validates a graph, orders the lists, runs the executions and
// writes every time text. This module holds the geometry of the canvas (the view math of the
// pan, the zoom and the cards), turns an edit of the person into the graph that `workflow.save`
// takes, and turns the facts into the words of a card.

import type {
  ExecutionState,
  ExecutionValue,
  NodeState,
  Verdict,
  WorkflowGraph,
  WorkflowNode,
  WorkflowProblem,
} from '@neohar/transport';

/** The size of a card on the canvas, in canvas units. */
export const NODE_W = 220;
export const NODE_H = 76;
/** The grid that a moved card snaps to. */
export const GRID = 20;
export const ZOOM_MIN = 0.25;
export const ZOOM_MAX = 2;

export interface Point {
  x: number;
  y: number;
}

/** The pan and the zoom: a canvas point `p` is at `p * zoom + (x, y)` on the screen. */
export interface View {
  x: number;
  y: number;
  zoom: number;
}

export const HOME: View = { x: 40, y: 40, zoom: 1 };

/** A screen point in canvas units. */
export function toCanvas(view: View, point: Point): Point {
  return { x: (point.x - view.x) / view.zoom, y: (point.y - view.y) / view.zoom };
}

/** The view after a zoom by `factor` that keeps the canvas point under `at` in its place. */
export function zoomAt(view: View, at: Point, factor: number): View {
  const zoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, view.zoom * factor));
  const anchor = toCanvas(view, at);
  return { zoom, x: at.x - anchor.x * zoom, y: at.y - anchor.y * zoom };
}

/** The view that shows every card in an area of `width` by `height`, at most at 100 %. */
export function fitView(nodes: readonly WorkflowNode[], width: number, height: number): View {
  if (nodes.length === 0 || width <= 0 || height <= 0) return HOME;
  const left = Math.min(...nodes.map((n) => n.x));
  const top = Math.min(...nodes.map((n) => n.y));
  const right = Math.max(...nodes.map((n) => n.x + NODE_W));
  const bottom = Math.max(...nodes.map((n) => n.y + NODE_H));
  const margin = 48;
  const zoom = Math.min(
    1,
    Math.max(
      ZOOM_MIN,
      Math.min((width - 2 * margin) / (right - left), (height - 2 * margin) / (bottom - top)),
    ),
  );
  return {
    zoom,
    x: (width - (right - left) * zoom) / 2 - left * zoom,
    y: (height - (bottom - top) * zoom) / 2 - top * zoom,
  };
}

export function snap(value: number): number {
  return Math.round(value / GRID) * GRID;
}

/** The input port of a card: the middle of its left side. */
export function portIn(node: Pick<WorkflowNode, 'x' | 'y'>): Point {
  return { x: node.x, y: node.y + NODE_H / 2 };
}

/** The output port of a card: the middle of its right side. */
export function portOut(node: Pick<WorkflowNode, 'x' | 'y'>): Point {
  return { x: node.x + NODE_W, y: node.y + NODE_H / 2 };
}

/** The curve of a connection from an output port to an input port, as SVG path data. */
export function edgePath(from: Point, to: Point): string {
  const bend = Math.max(40, Math.abs(to.x - from.x) / 2);
  return `M${from.x} ${from.y}C${from.x + bend} ${from.y} ${to.x - bend} ${to.y} ${to.x} ${to.y}`;
}

/** The middle of a connection, where its × goes. */
export function edgeMiddle(from: Point, to: Point): Point {
  return { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2 };
}

export interface Edge {
  key: string;
  from: string;
  to: string;
}

/** The connections of a graph: one for each input of each node whose input is a node. */
export function edges(graph: WorkflowGraph): Edge[] {
  const ids = new Set(graph.nodes.map((n) => n.id));
  return graph.nodes.flatMap((node) =>
    inputsOf(node)
      .filter((input) => ids.has(input))
      .map((input) => ({ key: `${input}>${node.id}`, from: input, to: node.id })),
  );
}

/** The ids of the nodes that connect to a node. */
export function inputsOf(node: WorkflowNode): string[] {
  return node.inputs ?? [];
}

/** The card under a canvas point, the one on top first. */
export function nodeAt(nodes: readonly WorkflowNode[], point: Point): WorkflowNode | undefined {
  for (let i = nodes.length - 1; i >= 0; i -= 1) {
    const node = nodes[i];
    if (
      node &&
      point.x >= node.x &&
      point.x <= node.x + NODE_W &&
      point.y >= node.y &&
      point.y <= node.y + NODE_H
    ) {
      return node;
    }
  }
  return undefined;
}

/** An id that no node of the graph has: `base`, else `base_2`, `base_3`... */
export function uniqueId(graph: WorkflowGraph, base: string): string {
  const clean =
    base
      .toLowerCase()
      .replace(/[^a-z0-9_]+/g, '_')
      .replace(/^[^a-z]+/, '')
      .replace(/_+$/, '')
      .slice(0, 24) || 'node';
  const taken = new Set(graph.nodes.map((n) => n.id));
  if (!taken.has(clean)) return clean;
  let n = 2;
  while (taken.has(`${clean}_${n}`)) n += 1;
  return `${clean}_${n}`;
}

/** The graph with a new node from a preset at a canvas point, connected from `from` if given. */
export function addNode(
  graph: WorkflowGraph,
  preset: { id: string; node: WorkflowNode },
  at: Point,
  from?: string,
): { graph: WorkflowGraph; id: string } {
  const id = uniqueId(graph, preset.id);
  const node: WorkflowNode = {
    ...preset.node,
    id,
    inputs: from ? [from] : [],
    x: snap(at.x),
    y: snap(at.y - NODE_H / 2),
  };
  return { graph: { ...graph, nodes: [...graph.nodes, node] }, id };
}

/**
 * The graph with a connection from `from` to `to`, or `null` when the connection cannot be: the
 * same node, a connection that is there, or a connection into the trigger. A loop is a problem
 * that the core shows.
 */
export function connect(graph: WorkflowGraph, from: string, to: string): WorkflowGraph | null {
  if (from === to) return null;
  const target = graph.nodes.find((n) => n.id === to);
  if (!target || target.kind === 'manual' || inputsOf(target).includes(from)) return null;
  if (!graph.nodes.some((n) => n.id === from)) return null;
  return {
    ...graph,
    nodes: graph.nodes.map((n) => (n.id === to ? { ...n, inputs: [...inputsOf(n), from] } : n)),
  };
}

export function disconnect(graph: WorkflowGraph, from: string, to: string): WorkflowGraph {
  return {
    ...graph,
    nodes: graph.nodes.map((n) =>
      n.id === to ? { ...n, inputs: inputsOf(n).filter((input) => input !== from) } : n,
    ),
  };
}

/** The graph without a node and its connections. The trigger stays. */
export function removeNode(graph: WorkflowGraph, id: string): WorkflowGraph {
  const node = graph.nodes.find((n) => n.id === id);
  if (!node || node.kind === 'manual') return graph;
  return {
    ...graph,
    nodes: graph.nodes
      .filter((n) => n.id !== id)
      .map((n) => ({ ...n, inputs: inputsOf(n).filter((input) => input !== id) })),
  };
}

export function moveNode(graph: WorkflowGraph, id: string, at: Point): WorkflowGraph {
  return {
    ...graph,
    nodes: graph.nodes.map((n) => (n.id === id ? { ...n, x: snap(at.x), y: snap(at.y) } : n)),
  };
}

export function updateNode(
  graph: WorkflowGraph,
  id: string,
  patch: Partial<WorkflowNode>,
): WorkflowGraph {
  return { ...graph, nodes: graph.nodes.map((n) => (n.id === id ? { ...n, ...patch } : n)) };
}

export function harnessName(harness: WorkflowNode['harness']): string {
  if (harness === 'claude') return 'Claude Code';
  if (harness === 'codex') return 'Codex';
  return 'No harness';
}

/** The title of a card: its title, else its id. */
export function nodeTitle(node: WorkflowNode): string {
  return (node.title ?? '').trim() || node.id;
}

/** The second line of a card: "Codex · gpt-5.5 · high · read-only". */
export function nodeLine(node: WorkflowNode, base?: string | null): string {
  if (node.kind === 'manual') return base ? `base ${base}` : 'Starts by hand';
  if (node.kind === 'report') {
    return node.summary_by ? `summary by ${harnessName(node.summary_by)}` : 'report.md';
  }
  const parts = [harnessName(node.harness)];
  if (node.model) parts.push(node.model);
  if (node.effort) parts.push(node.effort);
  parts.push(node.can_edit ? 'can edit' : 'read-only');
  return parts.join(' · ');
}

export function kindName(kind: WorkflowNode['kind']): string {
  if (kind === 'manual') return 'Trigger';
  if (kind === 'report') return 'Report';
  return 'Agent';
}

export function stateWord(state: NodeState): string {
  switch (state) {
    case 'waiting':
      return 'waiting';
    case 'queued':
      return 'queued';
    case 'working':
      return 'working';
    case 'needs_you':
      return 'needs you';
    case 'done':
      return 'done';
    case 'failed':
      return 'failed';
    case 'skipped':
      return 'skipped';
    case 'cancelled':
      return 'cancelled';
    default:
      return 'unknown';
  }
}

export function executionWord(state: ExecutionState): string {
  switch (state) {
    case 'running':
      return 'running';
    case 'paused':
      return 'paused';
    case 'done':
      return 'done';
    case 'cancelled':
      return 'cancelled';
    default:
      return 'unknown';
  }
}

export function verdictWord(verdict: Verdict | null | undefined): string | null {
  if (!verdict) return null;
  return verdict;
}

/** The state line of a card in an execution: "done · 6 min · warn". */
export function runLine(
  node: { state: NodeState; time_text?: string | null; verdict?: Verdict | null } | undefined,
): string {
  if (!node) return '';
  const parts = [stateWord(node.state)];
  if (node.time_text) parts.push(node.time_text);
  const verdict = verdictWord(node.verdict);
  if (verdict && node.state === 'done') parts.push(verdict);
  return parts.join(' · ');
}

/** The tone of a state, for the colour of a card and of a dot. */
export function tone(state: NodeState | ExecutionState): 'live' | 'caution' | 'danger' | 'quiet' {
  switch (state) {
    case 'done':
      return 'live';
    case 'working':
    case 'queued':
    case 'running':
      return 'caution';
    case 'needs_you':
    case 'failed':
    case 'paused':
      return 'danger';
    default:
      return 'quiet';
  }
}

/** The problems of one node, or of the workflow when `node` is null. */
export function problemsOf(
  problems: readonly WorkflowProblem[],
  node: string | null,
): WorkflowProblem[] {
  return problems.filter((p) => (p.node ?? null) === node);
}

/** The references that a prompt of a node can name, with the words of their chips. */
export function referenceChips(
  graph: WorkflowGraph,
  node: WorkflowNode,
): { label: string; text: string }[] {
  const trigger = graph.nodes.find((n) => n.kind === 'manual');
  const chips: { label: string; text: string }[] = [];
  if (trigger) {
    for (const field of ['branch', 'base', 'head', 'folder', 'notes']) {
      chips.push({ label: field, text: `{{${trigger.id}.${field}}}` });
    }
  }
  for (const input of inputsOf(node)) {
    const source = graph.nodes.find((n) => n.id === input);
    if (!source || source.kind === 'manual') continue;
    chips.push({ label: `${nodeTitle(source)} report`, text: `{{${source.id}.report}}` });
  }
  return chips;
}

/** A text with `insert` at a place, and the place after it. */
export function insertAt(
  text: string,
  start: number,
  end: number,
  insert: string,
): { text: string; at: number } {
  return { text: text.slice(0, start) + insert + text.slice(end), at: start + insert.length };
}

/** Whether an execution needs the person: a node that failed or waits for an answer. */
export function needsYou(execution: ExecutionValue): boolean {
  return execution.state === 'paused' || execution.nodes.some((node) => node.state === 'needs_you');
}

/** The last part of a path: the name of a folder. */
export function folderName(path: string): string {
  const parts = path.split('/').filter(Boolean);
  return parts[parts.length - 1] ?? path;
}
