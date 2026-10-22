// The gestures of a finger on a pane. The page turns them into what the mouse
// and the wheel of the desk send, and the core decides what they mean:
//
// | Gesture | What it does |
// |---|---|
// | Tap | The focus, and a click when the pane has a mouse protocol |
// | One finger up or down | Wheel notches, and a glide after the finger leaves |
// | Long press (500 ms) | A word selection, and a drag after it makes it longer |
// | Two fingers | They move the part of a larger grid |
// | Pinch | The font size of the phone, once when the fingers leave |
//
// The class keeps no state about the terminal. It knows the fingers and the time.

import type { Frames } from '../lib/frames';
import type { Timers } from '../lib/timing';

/** How long a finger stays down for a long press. */
export const LONG_PRESS_MS = 500;
/** How far a finger moves, in CSS pixels, before a press is a scroll and not a tap. */
export const SLOP = 10;
/** A change of the distance of two fingers above this share is a pinch, not a pan. */
const PINCH_SHARE = 0.12;
/** The glide stops below this speed, in CSS pixels for each millisecond. */
const GLIDE_STOP = 0.05;
/** The share of the speed that the glide keeps after 16 ms. */
const GLIDE_KEEP = 0.95;
/** The moves that the speed of a glide comes from. */
const SPEED_WINDOW_MS = 100;

export interface Point {
  x: number;
  y: number;
}

/** A finger on the screen: `Touch.identifier` and its place. */
export interface Finger extends Point {
  id: number;
}

/** What the gestures do. The surface of a pane gives them. */
export interface GestureHandlers {
  tap(at: Point): void;
  /** The content moves by `dy` CSS pixels: a positive value scrolls toward the end. */
  scroll(dy: number, at: Point): void;
  /** A long press at a point: a word selection starts there. */
  press(at: Point): void;
  /** The finger of a long press moves: the selection grows to the point. */
  extend(at: Point): void;
  /** The finger of a long press leaves. */
  release(): void;
  /** Two fingers moved by `dx` and `dy` CSS pixels since they came down. */
  pan(dx: number, dy: number): void;
  /** Two fingers pinch: `scale` is the distance now over the distance at the start. */
  pinch(scale: number, done: boolean): void;
}

type State =
  | { kind: 'idle' }
  | { kind: 'down'; start: Point; last: Point }
  | { kind: 'scroll'; last: Point }
  | { kind: 'press'; last: Point }
  | {
      kind: 'two';
      center: Point;
      distance: number;
      mode: 'unknown' | 'pan' | 'pinch';
      scale: number;
    };

function center(a: Point, b: Point): Point {
  return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
}

function distance(a: Point, b: Point): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

export class TouchGestures {
  private state: State = { kind: 'idle' };
  private timer: unknown = null;
  private moves: { y: number; time: number }[] = [];
  private glide: { speed: number; time: number; at: Point; frame: number } | null = null;

  constructor(
    private readonly handlers: GestureHandlers,
    private readonly timers: Timers,
    private readonly frames: Frames,
    private readonly now: () => number = () => performance.now(),
  ) {}

  /** Fingers came down. `fingers` is every finger that is on the pane now. */
  start(fingers: readonly Finger[]): void {
    this.stopGlide();
    this.cancelTimer();
    const [first, second] = fingers;
    if (first && second) {
      if (this.state.kind === 'press') this.handlers.release();
      this.state = {
        kind: 'two',
        center: center(first, second),
        distance: Math.max(distance(first, second), 1),
        mode: 'unknown',
        scale: 1,
      };
      return;
    }
    if (!first) return;
    const at = { x: first.x, y: first.y };
    this.state = { kind: 'down', start: at, last: at };
    this.moves = [{ y: at.y, time: this.now() }];
    this.timer = this.timers.set(() => {
      this.timer = null;
      if (this.state.kind !== 'down') return;
      this.state = { kind: 'press', last: this.state.last };
      this.handlers.press(this.state.last);
    }, LONG_PRESS_MS);
  }

  /** Fingers moved. */
  move(fingers: readonly Finger[]): void {
    const state = this.state;
    const [first, second] = fingers;
    if (state.kind === 'two' && first && second) {
      const now = center(first, second);
      const scale = distance(first, second) / state.distance;
      if (state.mode === 'unknown') {
        if (Math.abs(scale - 1) > PINCH_SHARE) state.mode = 'pinch';
        else if (distance(now, state.center) > SLOP) state.mode = 'pan';
      }
      if (state.mode === 'pan') this.handlers.pan(now.x - state.center.x, now.y - state.center.y);
      if (state.mode === 'pinch') {
        state.scale = scale;
        this.handlers.pinch(scale, false);
      }
      return;
    }
    if (!first) return;
    const at = { x: first.x, y: first.y };
    if (state.kind === 'down') {
      if (distance(at, state.start) <= SLOP) return;
      this.cancelTimer();
      this.state = { kind: 'scroll', last: state.start };
    }
    const current = this.state;
    if (current.kind === 'scroll') {
      // The content follows the finger: a finger that moves up scrolls toward the end.
      this.handlers.scroll(current.last.y - at.y, at);
      current.last = at;
      const time = this.now();
      this.moves.push({ y: at.y, time });
      while ((this.moves[0]?.time ?? time) < time - SPEED_WINDOW_MS) this.moves.shift();
    } else if (current.kind === 'press') {
      current.last = at;
      this.handlers.extend(at);
    }
  }

  /** Fingers left. `fingers` is every finger that is still on the pane. */
  end(fingers: readonly Finger[]): void {
    this.cancelTimer();
    const state = this.state;
    if (state.kind === 'two') {
      if (fingers.length > 0) return;
      if (state.mode === 'pinch') this.handlers.pinch(state.scale, true);
      this.state = { kind: 'idle' };
      return;
    }
    this.state = { kind: 'idle' };
    if (state.kind === 'down') this.handlers.tap(state.start);
    else if (state.kind === 'press') this.handlers.release();
    else if (state.kind === 'scroll') this.startGlide(state.last);
  }

  /** The surface goes away. */
  stop(): void {
    this.cancelTimer();
    this.stopGlide();
    this.state = { kind: 'idle' };
  }

  private cancelTimer(): void {
    if (this.timer !== null) this.timers.clear(this.timer);
    this.timer = null;
  }

  /** After the finger leaves, the scroll goes on and slows down on each frame. */
  private startGlide(at: Point): void {
    const first = this.moves[0];
    const last = this.moves[this.moves.length - 1];
    if (!first || !last || last.time <= first.time) return;
    // A finger that rested before it left stops the scroll.
    if (this.now() - last.time > SPEED_WINDOW_MS) return;
    // A finger that moved up gives a positive speed: the content goes on toward the end.
    const speed = (first.y - last.y) / (last.time - first.time);
    if (Math.abs(speed) < GLIDE_STOP * 4) return;
    const step = (): void => {
      const glide = this.glide;
      if (!glide) return;
      const time = this.now();
      const elapsed = Math.max(time - glide.time, 1);
      this.handlers.scroll(glide.speed * elapsed, glide.at);
      glide.speed *= GLIDE_KEEP ** (elapsed / 16);
      glide.time = time;
      if (Math.abs(glide.speed) < GLIDE_STOP) {
        this.glide = null;
        return;
      }
      glide.frame = this.frames.request(step);
    };
    this.glide = { speed, time: this.now(), at, frame: this.frames.request(step) };
  }

  private stopGlide(): void {
    if (this.glide) this.frames.cancel(this.glide.frame);
    this.glide = null;
  }
}

/** The fingers of a `TouchList`. */
export function fingersOf(list: TouchList): Finger[] {
  const out: Finger[] = [];
  for (let at = 0; at < list.length; at++) {
    const touch = list.item(at);
    if (touch) out.push({ id: touch.identifier, x: touch.clientX, y: touch.clientY });
  }
  return out;
}
