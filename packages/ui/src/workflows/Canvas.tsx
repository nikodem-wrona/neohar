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
  ExecutionNodeValue,
  ExecutionValue,
  WorkflowGraph,
  WorkflowNode,
  WorkflowProblem,
} from '@neohar/transport';
import {
  type KeyboardEvent,
  memo,
  type PointerEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';
import { isDrag } from '../app/drag';
import { useLatest, useSelect, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { createValue } from '../lib/value';
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

function capture(area: HTMLElement | null, event: { pointerId: number }): void {
  try {
    area?.setPointerCapture(event.pointerId);
  } catch {
    // A pointer that ended already has no capture.
  }
}

/** The card of a node. It renders again only when its node, its state or its problem changes. */
const NodeCard = memo(function NodeCard(props: {
  node: WorkflowNode;
  run: ExecutionNodeValue | undefined;
  /** The first problem of the node, while the canvas edits the graph. */
  issue: WorkflowProblem | undefined;
  selected: boolean;
  editing: boolean;
  base: string | null | undefined;
  onNodeDown: (event: PointerEvent, node: WorkflowNode) => void;
  onPortDown: (event: PointerEvent, node: WorkflowNode) => void;
}) {
  const { node, run, issue } = props;
  const state = run ? tone(run.state) : null;
  return (
    <div
      className={cn('wf-node glass', {
        selected: props.selected,
        problem: props.editing && issue !== undefined,
        [`tone-${state ?? 'none'}`]: true,
        trigger: node.kind === 'manual',
      })}
      style={{
        left: `${node.x}px`,
        top: `${node.y}px`,
        width: `${NODE_W}px`,
        height: `${NODE_H}px`,
      }}
      data-node={node.id}
      onPointerDown={(event) => props.onNodeDown(event, node)}
    >
      <div className="wf-node-head">
        <span className="wf-node-icon" aria-hidden="true">
          {ICONS[node.kind]}
        </span>
        <span className="wf-node-title">{nodeTitle(node)}</span>
      </div>
      <div className="wf-node-line">{nodeLine(node, props.base)}</div>
      <div className="wf-node-state">
        {run ? (
          <span>{runLine(run)}</span>
        ) : props.editing && issue ? (
          <span className="wf-node-problem">{issue.text}</span>
        ) : null}
      </div>
      {node.kind !== 'manual' ? <span className="wf-port in" aria-hidden="true" /> : null}
      <span
        className="wf-port out"
        title={props.editing ? 'Drag to connect, or to add a node' : undefined}
        onPointerDown={(event) => props.onPortDown(event, node)}
      />
    </div>
  );
});

export function Canvas(props: CanvasProps) {
  const area = useRef<HTMLDivElement>(null);
  // The handlers of the pointer read and write the view, the drag and the move at once, before
  // the next render shows them.
  const [view] = useState(() => createValue<View>(HOME));
  const [drag] = useState(() => createValue<Drag | null>(null));
  // Where a card is while the person drags it, until the core answers the save.
  const [moving] = useState(() => createValue<{ id: string; at: Point } | null>(null));
  const [edge, setEdge] = useState<string | null>(null);
  const shownView = useValue(view);
  const moved = useValue(moving);
  const panning = useSelect(drag, (current) => current?.kind === 'pan');
  const wiring = useSelect(drag, (current) => (current?.kind === 'wire' ? current : null));
  const latest = useLatest(props);
  const editing = props.execution === undefined;

  const nodes = props.graph.nodes.map((node) =>
    moved && node.id === moved.id ? { ...node, x: moved.at.x, y: moved.at.y } : node,
  );
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const lines = edges(props.graph).flatMap((e) => {
    const from = byId.get(e.from);
    const to = byId.get(e.to);
    return from && to ? [{ ...e, a: portOut(from), b: portIn(to) }] : [];
  });
  const runOf = (id: string) => props.execution?.nodes.find((node) => node.node === id);

  // A new graph from the core ends the drag of a card.
  const firstGraph = useRef(true);
  useEffect(() => {
    if (firstGraph.current) {
      firstGraph.current = false;
      return;
    }
    moving.set(null);
  }, [props.graph, moving]);

  const fit = useCallback((): void => {
    const element = area.current;
    if (!element) return;
    const box = element.getBoundingClientRect();
    view.set(fitView(latest.current.graph.nodes, box.width, box.height));
  }, [latest, view]);
  // A workflow or an execution that opens shows whole.
  const { fitKey } = props;
  useEffect(() => {
    queueMicrotask(fit);
  }, [fitKey, fit]);

  const local = useCallback((event: { clientX: number; clientY: number }): Point => {
    const box = area.current?.getBoundingClientRect();
    return { x: event.clientX - (box?.left ?? 0), y: event.clientY - (box?.top ?? 0) };
  }, []);
  const zoomBy = useCallback(
    (factor: number, at?: Point): void => {
      const box = area.current?.getBoundingClientRect();
      const center = at ?? { x: (box?.width ?? 0) / 2, y: (box?.height ?? 0) / 2 };
      view.set(zoomAt(view.get(), center, factor));
    },
    [view],
  );

  useEffect(() => {
    const element = area.current;
    // A wheel without a modifier pans, and a pinch or the mod key zooms. The listener is not
    // passive, so the page does not scroll.
    const onWheel = (event: WheelEvent): void => {
      event.preventDefault();
      if (event.ctrlKey || event.metaKey) {
        zoomBy(Math.exp(-event.deltaY / 200), local(event));
      } else {
        const before = view.get();
        view.set({
          ...before,
          x: before.x - event.deltaX,
          y: before.y - event.deltaY,
        });
      }
    };
    element?.addEventListener('wheel', onWheel, { passive: false });
    return () => element?.removeEventListener('wheel', onWheel);
  }, [local, zoomBy, view]);

  const onAreaDown = (event: PointerEvent): void => {
    if (event.button !== 0) return;
    capture(area.current, event);
    setEdge(null);
    drag.set({
      kind: 'pan',
      pointer: event.pointerId,
      start: local(event),
      origin: view.get(),
      moved: false,
    });
  };

  const onNodeDown = useCallback(
    (event: PointerEvent, node: WorkflowNode): void => {
      if (event.button !== 0) return;
      event.stopPropagation();
      setEdge(null);
      latest.current.onSelect(node.id);
      area.current?.focus();
      if (latest.current.execution !== undefined) return;
      capture(area.current, event);
      drag.set({
        kind: 'node',
        pointer: event.pointerId,
        id: node.id,
        start: local(event),
        origin: { x: node.x, y: node.y },
        moved: false,
      });
    },
    [latest, drag, local],
  );

  const onPortDown = useCallback(
    (event: PointerEvent, node: WorkflowNode): void => {
      if (event.button !== 0 || latest.current.execution !== undefined) return;
      event.stopPropagation();
      capture(area.current, event);
      drag.set({ kind: 'wire', pointer: event.pointerId, from: node.id, to: portOut(node) });
    },
    [latest, drag],
  );

  const onMoveEvent = (event: PointerEvent): void => {
    const current = drag.get();
    if (!current || current.pointer !== event.pointerId) return;
    const at = local(event);
    if (current.kind === 'pan') {
      const dx = at.x - current.start.x;
      const dy = at.y - current.start.y;
      if (!current.moved && !isDrag(dx, dy)) return;
      drag.set({ ...current, moved: true });
      view.set({ ...current.origin, x: current.origin.x + dx, y: current.origin.y + dy });
    } else if (current.kind === 'node') {
      const dx = (at.x - current.start.x) / view.get().zoom;
      const dy = (at.y - current.start.y) / view.get().zoom;
      if (!current.moved && !isDrag(dx, dy)) return;
      drag.set({ ...current, moved: true });
      moving.set({ id: current.id, at: { x: current.origin.x + dx, y: current.origin.y + dy } });
    } else {
      drag.set({ ...current, to: toCanvas(view.get(), at) });
    }
  };

  const onUp = (event: PointerEvent): void => {
    const current = drag.get();
    if (!current || current.pointer !== event.pointerId) return;
    drag.set(null);
    if (current.kind === 'pan') {
      if (!current.moved) props.onSelect(null);
    } else if (current.kind === 'node') {
      const moved = moving.get();
      if (current.moved && moved) props.onMove(current.id, moved.at);
      else moving.set(null);
    } else {
      const at = toCanvas(view.get(), local(event));
      const target = nodeAt(nodes, at);
      if (target && target.id !== current.from) props.onConnect(current.from, target.id);
      else if (!target) props.onPick({ x: event.clientX, y: event.clientY }, at, current.from);
    }
  };

  const onKey = (event: KeyboardEvent): void => {
    if (!editing) return;
    if (event.key === 'Delete' || event.key === 'Backspace') {
      const line = lines.find((l) => l.key === edge);
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
      const box = area.current?.getBoundingClientRect();
      const screen = {
        x: (box?.left ?? 0) + (box?.width ?? 0) / 2,
        y: (box?.top ?? 0) + (box?.height ?? 0) / 2,
      };
      const middle = { x: (box?.width ?? 0) / 2, y: (box?.height ?? 0) / 2 };
      props.onPick(screen, toCanvas(view.get(), middle), props.selected ?? undefined);
    }
  };

  const wireFrom = wiring ? byId.get(wiring.from) : undefined;
  const wire = wiring && wireFrom ? edgePath(portOut(wireFrom), wiring.to) : null;

  const zoomText = `${Math.round(shownView.zoom * 100)} %`;
  const size = 20 * shownView.zoom;
  const grid = {
    backgroundSize: `${size}px ${size}px`,
    backgroundPosition: `${shownView.x}px ${shownView.y}px`,
  };

  return (
    <div className="wf-canvas-wrap">
      <div
        className={cn('wf-canvas', { panning, readonly: !editing })}
        style={grid}
        tabIndex={0}
        role="application"
        aria-label={editing ? 'Workflow canvas' : 'Execution canvas'}
        ref={area}
        onPointerDown={onAreaDown}
        onPointerMove={onMoveEvent}
        onPointerUp={onUp}
        onDoubleClick={(event) => {
          // The canvas has the capture of the pointer, so it gets the double-click of a card.
          const found = nodeAt(nodes, toCanvas(view.get(), local(event)));
          if (found) props.onOpen(found.id);
        }}
        onContextMenu={(event) => {
          if (!editing) return;
          event.preventDefault();
          const at = toCanvas(view.get(), local(event));
          props.onPick({ x: event.clientX, y: event.clientY }, at);
        }}
        onPointerCancel={() => {
          drag.set(null);
          moving.set(null);
        }}
        onKeyDown={onKey}
      >
        <div
          className="wf-world"
          style={{
            transform: `translate(${shownView.x}px, ${shownView.y}px) scale(${shownView.zoom})`,
          }}
        >
          <svg className="wf-edges" aria-hidden="true">
            {lines.map((line) => {
              const from = runOf(line.from);
              const flowing = from?.state === 'done';
              return (
                <g
                  key={line.key}
                  className={cn({ selected: edge === line.key, flowing }) || undefined}
                >
                  <path className="wf-edge" d={edgePath(line.a, line.b)} />
                  <path
                    className="wf-edge-hit"
                    d={edgePath(line.a, line.b)}
                    onPointerDown={(event) => {
                      if (!editing) return;
                      event.stopPropagation();
                      props.onSelect(null);
                      setEdge(line.key);
                      area.current?.focus();
                    }}
                  />
                </g>
              );
            })}
            {wire ? <path className="wf-edge wf-wire" d={wire} /> : null}
          </svg>
          {editing
            ? lines
                .filter((l) => l.key === edge)
                .map((line) => {
                  const middle = edgeMiddle(line.a, line.b);
                  return (
                    <button
                      key={line.key}
                      type="button"
                      className="wf-edge-remove"
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
                })
            : null}
          {nodes.map((node) => (
            <NodeCard
              key={node.id}
              node={node}
              run={runOf(node.id)}
              issue={problemsOf(props.problems, node.id)[0]}
              selected={props.selected === node.id}
              editing={editing}
              base={props.base}
              onNodeDown={onNodeDown}
              onPortDown={onPortDown}
            />
          ))}
        </div>
      </div>
      <div className="wf-toolbar" role="toolbar" aria-label="Canvas">
        {editing ? (
          <button
            type="button"
            title="Add a node (Tab)"
            onClick={(event) => {
              const box = area.current?.getBoundingClientRect();
              const middle = { x: (box?.width ?? 0) / 2, y: (box?.height ?? 0) / 2 };
              props.onPick(
                { x: event.clientX, y: event.clientY - 300 },
                toCanvas(view.get(), middle),
              );
            }}
          >
            + Add
          </button>
        ) : null}
        <button type="button" title="Zoom out" onClick={() => zoomBy(1 / 1.25)}>
          −
        </button>
        <span className="wf-zoom">{zoomText}</span>
        <button type="button" title="Zoom in" onClick={() => zoomBy(1.25)}>
          +
        </button>
        <button type="button" title="Show the whole workflow (⌘0)" onClick={fit}>
          Fit
        </button>
        {editing ? (
          <button type="button" title="Put the nodes in columns" onClick={() => props.onTidy()}>
            Tidy up
          </button>
        ) : null}
      </div>
      {editing ? (
        <p className="wf-hint">
          Drag from a port to add a node · Tab or a right click adds one · Delete removes
        </p>
      ) : null}
    </div>
  );
}
