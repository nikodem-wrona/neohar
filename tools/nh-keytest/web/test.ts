// The page of the browser test. It dispatches recorded events at a surface, runs the keyboard layer
// of the UI on them, and returns the logical events. The test sends those to the encoder.

import {
  Keyboard,
  compositionText,
  type KeyboardOptions,
} from '../../../packages/ui/src/input/keyboard';
import type { LogicalInput } from '../../../packages/transport/src/generated';

/** A recorded event: the fields of a case's `events`. */
export interface DomEventSpec {
  type: string;
  key?: string;
  code?: string;
  location?: number;
  shift?: boolean;
  ctrl?: boolean;
  alt?: boolean;
  meta?: boolean;
  repeat?: boolean;
  is_composing?: boolean;
  key_code?: number;
  caps_lock?: boolean;
  num_lock?: boolean;
  alt_graph?: boolean;
  data?: string;
}

const surface = document.getElementById('surface');
if (!surface) throw new Error('the page has no surface');

function dispatchKey(spec: DomEventSpec): void {
  const event = new KeyboardEvent(spec.type, {
    key: spec.key ?? '',
    code: spec.code ?? '',
    location: spec.location ?? 0,
    repeat: spec.repeat ?? false,
    isComposing: spec.is_composing ?? false,
    shiftKey: spec.shift ?? false,
    ctrlKey: spec.ctrl ?? false,
    altKey: spec.alt ?? false,
    metaKey: spec.meta ?? false,
    bubbles: true,
    cancelable: true,
  });
  // A synthetic event has no `keyCode` and no lock state. Browsers report them like this.
  Object.defineProperty(event, 'keyCode', { value: spec.key_code ?? 0 });
  const states: Record<string, boolean> = {
    CapsLock: spec.caps_lock ?? false,
    NumLock: spec.num_lock ?? false,
    AltGraph: spec.alt_graph ?? false,
  };
  Object.defineProperty(event, 'getModifierState', {
    value: (name: string) => states[name] ?? false,
  });
  surface?.dispatchEvent(event);
}

window.keytest = {
  run(events: DomEventSpec[], options: KeyboardOptions): LogicalInput[] {
    const keyboard = new Keyboard(options);
    const out: LogicalInput[] = [];
    const key = (e: Event) => {
      const logical = keyboard.toLogical(e as KeyboardEvent);
      if (logical) out.push(logical);
    };
    const end = (e: Event) => {
      const text = compositionText(e as CompositionEvent);
      if (text) out.push(text);
    };
    surface.addEventListener('keydown', key);
    surface.addEventListener('keyup', key);
    surface.addEventListener('compositionend', end);
    try {
      for (const spec of events) {
        if (spec.type.startsWith('composition')) {
          surface.dispatchEvent(
            new CompositionEvent(spec.type, { data: spec.data ?? '', bubbles: true }),
          );
        } else {
          dispatchKey(spec);
        }
      }
    } finally {
      surface.removeEventListener('keydown', key);
      surface.removeEventListener('keyup', key);
      surface.removeEventListener('compositionend', end);
    }
    return out;
  },
};

declare global {
  interface Window {
    keytest: {
      run(events: DomEventSpec[], options: KeyboardOptions): LogicalInput[];
    };
  }
}
