// The canvas of a workflow, as in n8n: a card for each node and a curve for each connection. The
// person pans with a drag of the empty canvas or with two fingers, zooms with a pinch or the mod
// key and the wheel, drags a card to move it, and drags from the output port of a card to a card
// to connect them. A drag from a port to an empty place opens the node picker there.
//
// The cards are HTML in one layer that the pan and the zoom move and scale, so the text stays
// sharp. The connections are SVG paths in the same layer, under the cards. The page keeps only the
// drag in progress: a move or a connection goes to the core as a save of the graph, and the canvas
// draws the graph that the core gives back. An execution shows its graph read-only, with the state
// of each node.

import type {
  ExecutionValue,
  WorkflowGraph,
  WorkflowNode,
  WorkflowProblem,
} from '@neohar/transport';
import {
  createEffect,
  createMemo,
  createSignal,
  For,
  on,
  onCleanup,
  onMount,
  Show,
} from 'solid-js';
import { isDrag } from '../app/drag';
import {
  edgeMiddle,
  edgePath,
  edges,
  fitView,
  HOME,
  NODE_H,
  NODE_W,
  nodeAt,
  nodeLine,
  nodeTitle,
  type Point,
  portIn,
  portOut,
  problemsOf,
  runLine,
  tone,
  toCanvas,
  type View,
  zoomAt,
} from './model';

export interface CanvasProps {
  graph: WorkflowGraph;
  /** The workflow or the execution that the canvas shows. A new one fits the view. */
  fitKey: string;
  problems: readonly WorkflowProblem[];
  /** The execution that the canvas shows. Without it, the canvas edits the graph. */
  execution?: ExecutionValue | undefined;
  base?: string | null | undefined;
  selected: string | null;
  onSelect(node: string | null): void;
  onOpen(node: string): void;
  onMove(node: string, at: Point): void;
  onConnect(from: string, to: string): void;
  onDisconnect(from: string, to: string): void;
  onDelete(node: string): void;
  /** Opens the node picker at a screen point, for a node at a canvas point. */
  onPick(screen: Point, at: Point, from?: string): void;
  onTidy(): void;
}

type Drag =
  | { kind: 'pan'; pointer: number; start: Point; origin: View; moved: boolean }
  | { kind: 'node'; pointer: number; id: string; start: Point; origin: Point; moved: boolean }
  | { kind: 'wire'; pointer: number; from: string; to: Point };

const ICONS: Record<WorkflowNode['kind'], string> = { manual: '▶', agent: '◆', report: '≡' };

export function Canvas(props: CanvasProps) {
  let area: HTMLDivElement | undefined;
  const [view, setView] = createSignal<View>(HOME);
  const [drag, setDrag] = createSignal<Drag | null>(null);
  // Where a card is while the person drags it, until the core answers the save.
  const [moving, setMoving] = createSignal<{ id: string; at: Point } | null>(null);
  const [edge, setEdge] = createSignal<string | null>(null);
  const editing = () => props.execution === undefined;

  const nodes = createMemo(() => {
    const moved = moving();
    return props.graph.nodes.map((node) =>
      moved && node.id === moved.id ? { ...node, x: moved.at.x, y: moved.at.y } : node,
    );
  });
  const byId = createMemo(() => new Map(nodes().map((node) => [node.id, node])));
  const lines = createMemo(() =>
    edges(props.graph).flatMap((e) => {
      const from = byId().get(e.from);
      const to = byId().get(e.to);
      return from && to ? [{ ...e, a: portOut(from), b: portIn(to) }] : [];
    }),
  );
  const runOf = (id: string) => props.execution?.nodes.find((node) => node.node === id);

  // A new graph from the core ends the drag of a card.
  createEffect(
    on(
      () => props.graph,
      () => setMoving(null),
      { defer: true },
    ),
  );

  const fit = (): void => {
    if (!area) return;
    const box = area.getBoundingClientRect();
    setView(fitView(props.graph.nodes, box.width, box.height));
  };
  // A workflow or an execution that opens shows whole.
  createEffect(
    on(
      () => props.fitKey,
      () => queueMicrotask(fit),
    ),
  );

  const local = (event: { clientX: number; clientY: number }): Point => {
    const box = area?.getBoundingClientRect();
    return { x: event.clientX - (box?.left ?? 0), y: event.clientY - (box?.top ?? 0) };
  };
  const zoomBy = (factor: number, at?: Point): void => {
    const box = area?.getBoundingClientRect();
    const center = at ?? { x: (box?.width ?? 0) / 2, y: (box?.height ?? 0) / 2 };
    setView((before) => zoomAt(before, center, factor));
  };

  onMount(() => {
    // A wheel without a modifier pans, and a pinch or the mod key zooms. The listener is not
    // passive, so the page does not scroll.
    const onWheel = (event: WheelEvent): void => {
      event.preventDefault();
      if (event.ctrlKey || event.metaKey) {
        zoomBy(Math.exp(-event.deltaY / 200), local(event));
      } else {
        setView((before) => ({
          ...before,
          x: before.x - event.deltaX,
          y: before.y - event.deltaY,
        }));
      }
    };
    area?.addEventListener('wheel', onWheel, { passive: false });
    onCleanup(() => area?.removeEventListener('wheel', onWheel));
  });

  const capture = (event: PointerEvent): void => {
    try {
      area?.setPointerCapture(event.pointerId);
    } catch {
      // A pointer that ended already has no capture.
    }
  };

  const onAreaDown = (event: PointerEvent): void => {
    if (event.button !== 0) return;
    capture(event);
    setEdge(null);
    setDrag({
      kind: 'pan',
      pointer: event.pointerId,
      start: local(event),
      origin: view(),
      moved: false,
    });
  };

  const onNodeDown = (event: PointerEvent, node: WorkflowNode): void => {
    if (event.button !== 0) return;
    event.stopPropagation();
    setEdge(null);
    props.onSelect(node.id);
    area?.focus();
    if (!editing()) return;
    capture(event);
    setDrag({
      kind: 'node',
      pointer: event.pointerId,
      id: node.id,
      start: local(event),
      origin: { x: node.x, y: node.y },
      moved: false,
    });
  };

  const onPortDown = (event: PointerEvent, node: WorkflowNode): void => {
    if (event.button !== 0 || !editing()) return;
    event.stopPropagation();
    capture(event);
    setDrag({ kind: 'wire', pointer: event.pointerId, from: node.id, to: portOut(node) });
  };

  const onMoveEvent = (event: PointerEvent): void => {
    const current = drag();
    if (!current || current.pointer !== event.pointerId) return;
    const at = local(event);
    if (current.kind === 'pan') {
      const dx = at.x - current.start.x;
      const dy = at.y - current.start.y;
      if (!current.moved && !isDrag(dx, dy)) return;
      setDrag({ ...current, moved: true });
      setView({ ...current.origin, x: current.origin.x + dx, y: current.origin.y + dy });
    } else if (current.kind === 'node') {
      const dx = (at.x - current.start.x) / view().zoom;
      const dy = (at.y - current.start.y) / view().zoom;
      if (!current.moved && !isDrag(dx, dy)) return;
      setDrag({ ...current, moved: true });
      setMoving({ id: current.id, at: { x: current.origin.x + dx, y: current.origin.y + dy } });
    } else {
      setDrag({ ...current, to: toCanvas(view(), at) });
    }
  };

  const onUp = (event: PointerEvent): void => {
    const current = drag();
    if (!current || current.pointer !== event.pointerId) return;
    setDrag(null);
    if (current.kind === 'pan') {
      if (!current.moved) props.onSelect(null);
    } else if (current.kind === 'node') {
      const moved = moving();
      if (current.moved && moved) props.onMove(current.id, moved.at);
      else setMoving(null);
    } else {
      const at = toCanvas(view(), local(event));
      const target = nodeAt(nodes(), at);
      if (target && target.id !== current.from) props.onConnect(current.from, target.id);
      else if (!target) props.onPick({ x: event.clientX, y: event.clientY }, at, current.from);
    }
  };

  const onKey = (event: KeyboardEvent): void => {
    if (!editing()) return;
    if (event.key === 'Delete' || event.key === 'Backspace') {
      const line = lines().find((l) => l.key === edge());
      if (line) {
        event.preventDefault();
        props.onDisconnect(line.from, line.to);
        setEdge(null);
      } else if (props.selected) {
        event.preventDefault();
        props.onDelete(props.selected);
      }
    } else if ((event.metaKey || event.ctrlKey) && event.key === '0') {
      event.preventDefault();
      fit();
    } else if (event.key === 'Tab' && !event.shiftKey) {
      event.preventDefault();
      const box = area?.getBoundingClientRect();
      const screen = {
        x: (box?.left ?? 0) + (box?.width ?? 0) / 2,
        y: (box?.top ?? 0) + (box?.height ?? 0) / 2,
      };
      const middle = { x: (box?.width ?? 0) / 2, y: (box?.height ?? 0) / 2 };
      props.onPick(screen, toCanvas(view(), middle), props.selected ?? undefined);
    }
  };

  const wire = () => {
    const current = drag();
    if (current?.kind !== 'wire') return null;
    const from = byId().get(current.from);
    return from ? edgePath(portOut(from), current.to) : null;
  };

  const zoomText = () => `${Math.round(view().zoom * 100)} %`;
  const grid = () => {
    const v = view();
    const size = 20 * v.zoom;
    return {
      'background-size': `${size}px ${size}px`,
      'background-position': `${v.x}px ${v.y}px`,
    };
  };

  return (
    <div class="wf-canvas-wrap">
      <div
        class="wf-canvas"
        classList={{ panning: drag()?.kind === 'pan', readonly: !editing() }}
        style={grid()}
        tabIndex={0}
        role="application"
        aria-label={editing() ? 'Workflow canvas' : 'Execution canvas'}
        ref={(element) => (area = element)}
        onPointerDown={onAreaDown}
        onPointerMove={onMoveEvent}
        onPointerUp={onUp}
        onDblClick={(event) => {
          // The canvas has the capture of the pointer, so it gets the double-click of a card.
          const found = nodeAt(nodes(), toCanvas(view(), local(event)));
          if (found) props.onOpen(found.id);
        }}
        onContextMenu={(event) => {
          if (!editing()) return;
          event.preventDefault();
          const at = toCanvas(view(), local(event));
          props.onPick({ x: event.clientX, y: event.clientY }, at);
        }}
        onPointerCancel={() => {
          setDrag(null);
          setMoving(null);
        }}
        onKeyDown={onKey}
      >
        <div
          class="wf-world"
          style={{ transform: `translate(${view().x}px, ${view().y}px) scale(${view().zoom})` }}
        >
          <svg class="wf-edges" aria-hidden="true">
            <For each={lines()}>
              {(line) => {
                const from = () => runOf(line.from);
                const flowing = () => from()?.state === 'done';
                return (
                  <g classList={{ selected: edge() === line.key, flowing: flowing() }}>
                    <path class="wf-edge" d={edgePath(line.a, line.b)} />
                    <path
                      class="wf-edge-hit"
                      d={edgePath(line.a, line.b)}
                      onPointerDown={(event) => {
                        if (!editing()) return;
                        event.stopPropagation();
                        props.onSelect(null);
                        setEdge(line.key);
                        area?.focus();
                      }}
                    />
                  </g>
                );
              }}
            </For>
            <Show when={wire()}>{(d) => <path class="wf-edge wf-wire" d={d()} />}</Show>
          </svg>
          <Show when={editing()}>
            <For each={lines().filter((l) => l.key === edge())}>
              {(line) => {
                const middle = edgeMiddle(line.a, line.b);
                return (
                  <button
                    type="button"
                    class="wf-edge-remove"
                    style={{ left: `${middle.x}px`, top: `${middle.y}px` }}
                    title="Remove the connection (Delete)"
                    onPointerDown={(event) => event.stopPropagation()}
                    onClick={() => {
                      props.onDisconnect(line.from, line.to);
                      setEdge(null);
                    }}
                  >
                    ×
                  </button>
                );
              }}
            </For>
          </Show>
          <For each={nodes()}>
            {(node) => {
              const run = () => runOf(node.id);
              const issues = () => problemsOf(props.problems, node.id);
              const state = () => {
                const found = run();
                return found ? tone(found.state) : null;
              };
              return (
                <div
                  class="wf-node glass"
                  classList={{
                    selected: props.selected === node.id,
                    problem: editing() && issues().length > 0,
                    [`tone-${state() ?? 'none'}`]: true,
                    trigger: node.kind === 'manual',
                  }}
                  style={{
                    left: `${node.x}px`,
                    top: `${node.y}px`,
                    width: `${NODE_W}px`,
                    height: `${NODE_H}px`,
                  }}
                  data-node={node.id}
                  onPointerDown={(event) => onNodeDown(event, node)}
                >
                  <div class="wf-node-head">
                    <span class="wf-node-icon" aria-hidden="true">
                      {ICONS[node.kind]}
                    </span>
                    <span class="wf-node-title">{nodeTitle(node)}</span>
                  </div>
                  <div class="wf-node-line">{nodeLine(node, props.base)}</div>
                  <div class="wf-node-state">
                    <Show
                      when={run()}
                      fallback={
                        <Show when={editing() && issues()[0]}>
                          {(issue) => <span class="wf-node-problem">{issue().text}</span>}
                        </Show>
                      }
                    >
                      {(found) => <span>{runLine(found())}</span>}
                    </Show>
                  </div>
                  <Show when={node.kind !== 'manual'}>
                    <span class="wf-port in" aria-hidden="true" />
                  </Show>
                  <span
                    class="wf-port out"
                    title={editing() ? 'Drag to connect, or to add a node' : undefined}
                    onPointerDown={(event) => onPortDown(event, node)}
                  />
                </div>
              );
            }}
          </For>
        </div>
      </div>
      <div class="wf-toolbar" role="toolbar" aria-label="Canvas">
        <Show when={editing()}>
          <button
            type="button"
            title="Add a node (Tab)"
            onClick={(event) => {
              const box = area?.getBoundingClientRect();
              const middle = { x: (box?.width ?? 0) / 2, y: (box?.height ?? 0) / 2 };
              props.onPick({ x: event.clientX, y: event.clientY - 300 }, toCanvas(view(), middle));
            }}
          >
            + Add
          </button>
        </Show>
        <button type="button" title="Zoom out" onClick={() => zoomBy(1 / 1.25)}>
          −
        </button>
        <span class="wf-zoom">{zoomText()}</span>
        <button type="button" title="Zoom in" onClick={() => zoomBy(1.25)}>
          +
        </button>
        <button type="button" title="Show the whole workflow (⌘0)" onClick={fit}>
          Fit
        </button>
        <Show when={editing()}>
          <button type="button" title="Put the nodes in columns" onClick={() => props.onTidy()}>
            Tidy up
          </button>
        </Show>
      </div>
      <Show when={editing()}>
        <p class="wf-hint">
          Drag from a port to add a node · Tab or a right click adds one · Delete removes
        </p>
      </Show>
    </div>
  );
}
