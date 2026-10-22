// The surface of a pane: the box that the renderer paints into, and the input of the pane. The
// canvas of the window shows through the box. A hidden `<textarea>` follows the cursor
// cell and owns the keyboard, the input method and the paste. The page turns what the
// user does into logical events with cell coordinates, and the core decides what they mean.
//
// On a phone, the text of the soft keyboard comes from the field (`input/soft`),
// and a `keydown` with a physical key comes from a hardware keyboard. A tap on the pane in focus
// opens the keyboard, and the window then claims the size of the pane (`pane.claim`). The fingers
// scroll, select, move the part of a larger grid and change the font size (`input/touch`).

import type { PaneId } from '@neohar/transport';
import {
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
  Show,
  untrack,
} from 'solid-js';
import { useApp } from '../app/context';
import { clampPhoneFont, fontOf } from '../app/preferences';
import { isCopyChord, isPasteChord, readClipboard, writeClipboard } from '../input/clipboard';
import { compositionText } from '../input/keyboard';
import { BACKSPACE, chordOf, ENTER, endsWord, FieldText, stickyMods } from '../input/soft';
import { fingersOf, type Point, TouchGestures } from '../input/touch';
import { pageFrames } from '../lib/frames';
import { realTimers } from '../lib/timing';
import {
  bypassesMouseMode,
  buttonName,
  cellAt,
  modsOf,
  mouseProtocol,
  selectMode,
  WheelAccumulator,
} from '../input/mouse';
import type { SelectMode } from '../input/mouse';
import { Scrollbar } from './Scrollbar';

/** A selection that the pointer is making. */
interface Drag {
  anchor: { col: number; line: number };
  mode: SelectMode;
  moved: boolean;
  /** A plain click clears the selection that is there, unless the pointer moves. */
  clearsOnRelease: boolean;
}

export function Surface(props: { id: PaneId; focused: boolean }) {
  const {
    stores,
    transport,
    run,
    report,
    render,
    input,
    keyboard,
    platform,
    view,
    form,
    windowId,
  } = useApp();
  const phone = form === 'phone';
  const pane = () => stores.panes.state.byKey[props.id];
  const renderId = createMemo(() => pane()?.render_id);
  let box: HTMLDivElement | undefined;
  let field: HTMLTextAreaElement | undefined;
  /** The text of the field that went to the pane, on a phone. */
  const typed = new FieldText();
  const mountedId = untrack(() => props.id);
  const restartField = (): void => {
    typed.clear();
    if (field) field.value = '';
  };

  // The renderer of the window paints the pane into the box.
  createEffect(() => {
    const id = renderId();
    if (render === undefined || id === undefined || box === undefined) return;
    // The size of the projection is read once. A new size must not attach the pane again: the
    // decoder would lose the screen of the pane, and the core would not send it again.
    const known = untrack(() => {
      const row = pane();
      return row ? { cols: row.cols, rows: row.rows } : undefined;
    });
    // The box of the pane around the surface has the padding, and the background fills it.
    onCleanup(render.attach(id, props.id, box, known, box.parentElement ?? box));
  });

  // The core says which pane has the focus, and the page moves the DOM focus to the input.
  createEffect(() => {
    // The palette, the search, the question of a close, the field of a new name and the task
    // dialog have the focus while they are open, and the pane takes it back when they close.
    const overlay =
      view.paletteOpen() ||
      view.searchOpen() ||
      view.closing() !== null ||
      view.renaming() !== null ||
      view.taskDialog() !== null;
    // A phone opens its keyboard only for a tap: a focus from code would cover half of the screen
    // when the person only looks.
    if (phone) return;
    if (props.focused && !overlay && field && document.activeElement !== field) {
      field.focus({ preventScroll: true });
    }
  });

  // The input follows the cursor cell, so that the preview of an input method lies there.
  createEffect(() => {
    const id = renderId();
    if (render === undefined || id === undefined) return;
    const place = (): void => {
      if (!field) return;
      const cell = render.cell;
      const raw = render.decoder.raw;
      // The part of a larger grid that the box shows.
      const crop = render.crop(id);
      field.style.left = `${(raw.cursor_x(id) - crop.left) * cell.w}px`;
      field.style.top = `${(raw.cursor_y(id) - crop.top) * cell.h}px`;
      field.style.height = `${cell.h}px`;
      // Safari zooms into a field with a font under 16 px.
      field.style.fontSize = phone ? '16px' : `${cell.h * 0.8}px`;
    };
    place();
    onCleanup(render.watch(id, place));
  });

  // --- the keyboard --------------------------------------------------------------------------

  const onKey = (e: KeyboardEvent): void => {
    // A key of the soft keyboard of a phone has no physical key: its text comes from the field.
    // A hardware keyboard has the path of the desk.
    if (phone && e.code === '') return;
    if (e.type === 'keydown') {
      if (isCopyChord(e, platform)) {
        e.preventDefault();
        if (hasSelection()) copy();
        return;
      }
      // The browser makes a `paste` event from this key. The text goes in `onPaste`.
      if (isPasteChord(e, platform)) return;
    }
    const logical = keyboard.toLogical(e);
    // No event: an input method owns the key, or it is a dead key. The browser goes on.
    if (!logical) return;
    e.preventDefault();
    if (logical.kind === 'release') {
      const id = renderId();
      if (render === undefined || id === undefined || !render.decoder.raw.wants_key_release(id)) {
        return;
      }
    }
    // A hardware key on a phone: the text of the field no longer matches the pane.
    if (phone && logical.kind === 'press') restartField();
    input.send(props.id, logical);
  };

  const onCompositionEnd = (e: CompositionEvent): void => {
    // On a phone the field holds the text of the composition, and the change of the field sends it.
    if (phone) {
      sendField();
      return;
    }
    const text = compositionText(e);
    if (text) input.send(props.id, text);
    if (field) field.value = '';
  };

  /** Sends what changed in the field of a phone since the last time. */
  const sendField = (): void => {
    if (!field) return;
    for (const event of typed.change(field.value)) input.send(props.id, event);
    if (endsWord(field.value)) restartField();
  };

  // The rules of `beforeinput` on a phone. The other kinds of input change the field, and
  // `input` sends the change.
  const onBeforeInput = (e: InputEvent): void => {
    if (!phone || e.isComposing || !field) return;
    const type = e.inputType;
    if (type === 'insertLineBreak' || type === 'insertParagraph') {
      e.preventDefault();
      sendField();
      input.send(props.id, ENTER);
      restartField();
      return;
    }
    if (type === 'deleteContentBackward' && field.value === '') {
      e.preventDefault();
      input.send(props.id, BACKSPACE);
      return;
    }
    // Ctrl or Alt of the key bar: the next character is a chord, as on a hardware keyboard.
    const mods = stickyMods(view.barCtrl(), view.barAlt());
    if (mods.length > 0 && type === 'insertText' && e.data && Array.from(e.data).length === 1) {
      e.preventDefault();
      input.send(props.id, chordOf(e.data, mods));
      view.usedBarMods();
      restartField();
    }
  };

  const onPaste = (e: ClipboardEvent): void => {
    e.preventDefault();
    const text = e.clipboardData?.getData('text/plain') ?? '';
    if (text === '') return;
    if (phone) restartField();
    pasteText(text);
  };

  const pasteText = (text: string): void => {
    input.paste(props.id, text);
  };

  /**
   * Copies the selection. The desk writes the clipboard through its host. A phone writes its own
   * clipboard in the tap, with the text that the core gives.
   */
  const copy = (): void => {
    if (!phone) {
      run('pane.copy', { pane: props.id });
      return;
    }
    const text = transport
      .call('pane.copy', { pane: props.id, return_text: true })
      .then((result) => result.text ?? '');
    writeClipboard(text).catch(report);
  };

  // --- the mouse -----------------------------------------------------------------------------

  const hasSelection = (): boolean => {
    const id = renderId();
    return render !== undefined && id !== undefined && render.decoder.raw.selection_mode(id) !== 0;
  };

  /** The protocol that the program asked for. The frame has it with the screen it shows. */
  const protocol = () => {
    const id = renderId();
    return render === undefined || id === undefined
      ? 'none'
      : mouseProtocol(render.decoder.raw.modes(id));
  };
  const reporting = (): boolean => protocol() !== 'none';

  /** The cell under the pointer, and the absolute line of its row. */
  const locate = (e: MouseEvent) => locatePoint({ x: e.clientX, y: e.clientY });

  /** The cell under a point of the page, and the absolute line of its row. */
  const locatePoint = (point: Point) => {
    const id = renderId();
    if (render === undefined || id === undefined || box === undefined) return null;
    const raw = render.decoder.raw;
    // A grid that is larger than the box shows a part of it: the box starts there.
    const crop = render.crop(id);
    const shown = box.getBoundingClientRect();
    const origin = {
      left: shown.left - crop.left * render.cell.w,
      top: shown.top - crop.top * render.cell.h,
    };
    const cell = cellAt(point, origin, render.cell, {
      cols: raw.cols(id),
      rows: raw.rows(id),
    });
    return { ...cell, line: raw.viewport_top(id) + cell.row };
  };

  let drag: Drag | null = null;
  let reported: ReturnType<typeof buttonName> | null = null;

  const onMouseDown = (e: MouseEvent): void => {
    // The default action of `mousedown` moves the focus to the body after this handler. The
    // input would lose the keyboard, so the page cancels it and gives the input the focus.
    e.preventDefault();
    field?.focus({ preventScroll: true });
    const at = locate(e);
    if (!at) return;
    if (reporting() && !bypassesMouseMode(e)) {
      reported = buttonName(e.button);
      input.mouse(props.id, {
        type: 'mouse',
        kind: 'down',
        button: reported,
        col: at.col,
        row: at.row,
        mods: modsOf(e),
      });
      watchPointer();
      return;
    }
    if (e.button !== 0) return;
    const mode = selectMode(e.detail, e.altKey);
    const anchor = { col: at.col, line: at.line };
    // A single click selects nothing until the pointer moves. A double or triple click selects
    // a word or a line at once.
    const plain = mode === 'simple';
    drag = { anchor, mode, moved: !plain, clearsOnRelease: plain && hasSelection() };
    if (!plain) input.select(props.id, { anchor, head: anchor, mode });
    watchPointer();
  };

  const onPointerMove = (e: MouseEvent): void => {
    const at = locate(e);
    if (!at) return;
    if (drag) {
      const head = { col: at.col, line: at.line };
      if (!drag.moved && head.col === drag.anchor.col && head.line === drag.anchor.line) return;
      drag.moved = true;
      input.select(props.id, { anchor: drag.anchor, head, mode: drag.mode });
    } else if (reported !== null) {
      input.move(props.id, { col: at.col, row: at.row, mods: modsOf(e), button: reported });
    }
  };

  const onPointerUp = (e: MouseEvent): void => {
    document.removeEventListener('mousemove', onPointerMove);
    document.removeEventListener('mouseup', onPointerUp);
    if (drag) {
      if (!drag.moved && drag.clearsOnRelease) {
        run('pane.select_clear', { pane: props.id });
      }
      drag = null;
      input.flush();
    }
    if (reported !== null) {
      const at = locate(e);
      if (at) {
        input.mouse(props.id, {
          type: 'mouse',
          kind: 'up',
          button: reported,
          col: at.col,
          row: at.row,
          mods: modsOf(e),
        });
      }
      reported = null;
    }
  };

  /** The pointer can leave the pane during a drag, so the page follows it on the document. */
  const watchPointer = (): void => {
    document.addEventListener('mousemove', onPointerMove);
    document.addEventListener('mouseup', onPointerUp);
  };

  // A pointer that moves over the pane with no button down, for a program that asked for every move.
  const onHover = (e: MouseEvent): void => {
    if (e.buttons !== 0 || bypassesMouseMode(e) || protocol() !== 'any_event') return;
    const at = locate(e);
    if (at) input.move(props.id, { col: at.col, row: at.row, mods: modsOf(e), button: 'none' });
  };

  const wheel = new WheelAccumulator();
  const onWheel = (e: WheelEvent): void => {
    e.preventDefault();
    const at = locate(e);
    if (!at || render === undefined) return;
    const ready = wheel.add(e.deltaX, e.deltaY, e.deltaMode, render.cell.h);
    if (ready.x === 0 && ready.y === 0) return;
    input.wheel(props.id, { col: at.col, row: at.row, x: ready.x, y: ready.y, mods: modsOf(e) });
  };

  // --- the fingers of a phone ----------------------------------------------------

  const [selecting, setSelecting] = createSignal(false);
  /** The pane had the focus when the finger came down: a tap opens the keyboard. */
  let focusedAtStart = false;
  /** The move of the part of a larger grid, in cells, and its value when two fingers came down. */
  const pan = { left: 0, top: 0 };
  let panBase = { left: 0, top: 0 };
  let pressAnchor: { col: number; line: number } | null = null;
  const gestures = new TouchGestures(
    {
      tap: (point) => {
        const at = locatePoint(point);
        if (at && reporting()) {
          for (const kind of ['down', 'up'] as const) {
            input.mouse(props.id, {
              type: 'mouse',
              kind,
              button: 'left',
              col: at.col,
              row: at.row,
              mods: [],
            });
          }
        }
        if (hasSelection()) {
          run('pane.select_clear', { pane: props.id });
          setSelecting(false);
        }
        // The handler of the tap gives the field the focus: Safari shows the keyboard only then.
        if (focusedAtStart) field?.focus({ preventScroll: true });
      },
      scroll: (dy, point) => {
        const at = locatePoint(point);
        if (!at || render === undefined) return;
        const ready = wheel.add(0, dy, 0, render.cell.h);
        if (ready.y !== 0) {
          input.wheel(props.id, { col: at.col, row: at.row, x: 0, y: ready.y, mods: [] });
        }
      },
      press: (point) => {
        const at = locatePoint(point);
        if (!at) return;
        pressAnchor = { col: at.col, line: at.line };
        input.select(props.id, { anchor: pressAnchor, head: pressAnchor, mode: 'word' });
        input.flush();
        setSelecting(true);
      },
      extend: (point) => {
        const at = locatePoint(point);
        if (!at || !pressAnchor) return;
        input.select(props.id, {
          anchor: pressAnchor,
          head: { col: at.col, line: at.line },
          mode: 'word',
        });
      },
      release: () => {
        pressAnchor = null;
        input.flush();
      },
      pan: (dx, dy) => {
        const id = renderId();
        if (render === undefined || id === undefined) return;
        pan.left = panBase.left - Math.round(dx / render.cell.w);
        pan.top = panBase.top - Math.round(dy / render.cell.h);
        render.setPan(id, pan.left, pan.top);
      },
      pinch: (scale, done) => {
        if (!done) return;
        const size = fontOf(stores.settings.state.byKey, 'phone').px ?? 12;
        const next = clampPhoneFont(size * scale);
        if (next !== size) {
          run('config.set', { key: 'phone.font_size', value: next });
        }
      },
    },
    realTimers,
    pageFrames,
  );
  const onTouchStart = (e: TouchEvent): void => {
    // The page handles the fingers. No mouse event follows, and the page does not scroll.
    e.preventDefault();
    if (e.touches.length === 1) focusedAtStart = props.focused;
    // A pan of two fingers goes on from the part that shows now.
    if (e.touches.length === 2) panBase = { left: pan.left, top: pan.top };
    gestures.start(fingersOf(e.touches));
  };
  const onTouchMove = (e: TouchEvent): void => {
    e.preventDefault();
    gestures.move(fingersOf(e.touches));
  };
  const onTouchEnd = (e: TouchEvent): void => {
    gestures.end(fingersOf(e.touches));
  };

  // --- the field on a phone -------------------------------------------------------------------

  const onFocus = (): void => {
    if (!phone || !field) return;
    view.setTyping({
      pane: props.id,
      restart: restartField,
      hide: () => field?.blur(),
      focus: () => field?.focus({ preventScroll: true }),
    });
    // The keyboard takes the bottom of the screen: this window gets the size of the PTY before the
    // first key, so the agent draws for the space that is left.
    // A pane that ended takes no size, and it shows that it ended.
    void transport.call('pane.claim', { pane: props.id, window: windowId }).catch(() => undefined);
  };
  const onBlur = (e: FocusEvent): void => {
    keyboard.reset();
    if (!phone) return;
    // A button of the key bar keeps the keyboard: the field gets the focus back.
    const next = e.relatedTarget as Element | null;
    if (next?.closest('.key-bar')) return;
    if (view.typing()?.pane === props.id) view.setTyping(null);
    restartField();
  };

  onMount(() => {
    // A wheel handler must be able to cancel the scroll of the page, so it is not passive.
    box?.addEventListener('wheel', onWheel, { passive: false });
    if (phone && box) {
      box.addEventListener('touchstart', onTouchStart, { passive: false });
      box.addEventListener('touchmove', onTouchMove, { passive: false });
      box.addEventListener('touchend', onTouchEnd);
      box.addEventListener('touchcancel', onTouchEnd);
    }
  });
  onCleanup(() => {
    box?.removeEventListener('wheel', onWheel);
    box?.removeEventListener('touchstart', onTouchStart);
    box?.removeEventListener('touchmove', onTouchMove);
    box?.removeEventListener('touchend', onTouchEnd);
    box?.removeEventListener('touchcancel', onTouchEnd);
    gestures.stop();
    document.removeEventListener('mousemove', onPointerMove);
    document.removeEventListener('mouseup', onPointerUp);
    // The id of the mount: a prop read in a cleanup is stale when a split replaced the node.
    if (view.typing()?.pane === mountedId) view.setTyping(null);
  });

  return (
    <div
      class="pane-surface"
      ref={(element) => (box = element)}
      onMouseDown={onMouseDown}
      onMouseMove={onHover}
    >
      <textarea
        class="pane-input"
        ref={(element) => (field = element)}
        aria-label={`${pane()?.title ?? 'pane'} input`}
        autocapitalize="off"
        autocomplete="off"
        autocorrect="off"
        spellcheck={false}
        enterkeyhint="send"
        onKeyDown={onKey}
        onKeyUp={onKey}
        onCompositionEnd={onCompositionEnd}
        onPaste={onPaste}
        onFocus={onFocus}
        onBlur={onBlur}
        onBeforeInput={onBeforeInput}
        onInput={(e) => {
          if ((e as InputEvent).isComposing) return;
          // On a phone the field keeps the text of the word, and its change goes to the pane.
          if (phone) {
            sendField();
            return;
          }
          // The keys do not insert text. An input method does, and `compositionend` takes it.
          e.currentTarget.value = '';
        }}
      />
      <Show when={phone && selecting()}>
        {/* The bar of a selection that a long press made. */}
        <div class="selection-bar" role="toolbar" aria-label="Selection">
          <button
            type="button"
            onPointerDown={(e) => e.preventDefault()}
            onClick={() => {
              copy();
              setSelecting(false);
            }}
          >
            Copy
          </button>
          <button
            type="button"
            onPointerDown={(e) => e.preventDefault()}
            onClick={() => {
              setSelecting(false);
              readClipboard().then(pasteText, (failure: unknown) =>
                console.warn('the paste failed', failure),
              );
            }}
          >
            Paste
          </button>
        </div>
      </Show>
      <Scrollbar id={props.id} renderId={renderId()} />
    </div>
  );
}
