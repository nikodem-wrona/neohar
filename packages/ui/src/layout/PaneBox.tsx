// A pane of the layout: a title strip over the body. On a phone the strip has the button ⋯ in place
// of float and zoom: the menu has the splits, zoom, float, Fit, Attach a file, Copy, Paste and
// Close. The strip says what the board does not:
// the dot of the state, the handle when `ui.show_handles` is on, the state in four
// letters only for `WAIT` and `ERR`, the badge of the harness with its marks, the title when it
// does not only name the harness, and the marker of unconfirmed input. When another window
// has the size of the PTY, the strip has the mark Fit, which takes the size (`pane.claim`).
// The pane in focus
// has a ring in the gap around it. The strip has the buttons for the details, float, zoom and
// close. The details are a popover (`details.ts`): the working directory, the cost, the tokens,
// the model, the version, the session and the size.
// A tiled pane drags by its strip onto a tab (`pane.move`). A row of the Files tab drops onto a
// pane, and the core pastes its path (`pane.insert_paths`). The surface of the pane goes
// into `.pane-body`.

import type { PaneId, TabValue } from '@neohar/transport';
import { Fragment, memo, useCallback, useRef, useState } from 'react';
import { badgeOf, harnessName } from '../agents/badge';
import { HarnessBadge } from '../agents/HarnessBadge';
import { useApp } from '../app/context';
import { paneName, showHandles } from '../app/preferences';
import { dragged, dropTarget, pressToDrag } from '../app/drag';
import { Badge } from '../kit/Badge';
import { Button } from '../kit/Button';
import { Popover } from '../kit/Popover';
import { useLatest, useSelect, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { Surface } from '../pane/Surface';
import { readClipboard, writeClipboard } from '../input/clipboard';
import { requestClose } from '../shell/close';
import { detailsTitle, paneDetails, stripTitle } from './details';
import { tabPanes } from './tree';

/** How far a finger moves sideways on the strip of the single view to show another pane. */
const SWIPE = 48;

export const PaneBox = memo(function PaneBox(props: {
  id: PaneId;
  tab: TabValue;
  single?: boolean;
}) {
  // `run` of the app is `call` here: `run` is the run of the agent in the pane.
  const {
    stores,
    transport,
    run: call,
    report,
    windowId,
    view,
    input,
    form,
    upload,
    toasts,
  } = useApp();
  const phone = form === 'phone';
  const attach = useRef<HTMLInputElement>(null);
  const pane = useSelect(stores.panes, (state) => state.byKey[props.id]);
  const handles = useSelect(stores.settings, (state) => showHandles(state.byKey));
  /** The run of an agent in the pane. */
  const runId = pane?.run;
  const run = useSelect(stores.agents, (state) => (runId ? state.byKey[runId] : undefined));
  const focused = useSelect(
    stores.windows,
    (state) => state.byKey[windowId]?.focused_pane === props.id,
  );
  const zoomed = props.tab.zoomed === props.id;
  const floating = props.tab.floating?.some((item) => item.pane === props.id) === true;
  const focus = () => call('pane.focus', { pane: props.id, window: windowId });
  /** The state of a pane whose program ended. */
  const exited = pane?.liveness?.state === 'exited' ? pane.liveness : undefined;
  const restart = () => call('pane.restart', { pane: props.id });
  /** Another window has the size of the PTY: the grid can be larger than the box. */
  const owner = pane?.geometry_owner;
  const otherSize = owner !== undefined && owner !== null && owner !== windowId;
  const fit = () => call('pane.claim', { pane: props.id, window: windowId });
  // The single view of a phone: a dot for each pane of the tab, and a swipe on the strip shows the
  // next or the previous one. The core draws the pane that has the focus.
  const siblings = props.single ? tabPanes(props.tab) : [];
  const place = siblings.indexOf(props.id);
  const show = (step: number): void => {
    const next = siblings[place + step];
    if (next) call('pane.focus', { pane: next, window: windowId });
  };
  const swipeX = useRef<number | null>(null);
  const toggleZoom = () =>
    call('layout.zoom', zoomed ? { tab: props.tab.id } : { tab: props.tab.id, pane: props.id });
  const toggleFloat = () => call(floating ? 'pane.unfloat' : 'pane.float', { pane: props.id });
  const close = (): void => {
    requestClose(transport, {
      kind: 'pane',
      id: props.id,
      name: paneName(pane, handles) ?? props.id,
    })
      .then((closing) => view.closing.set(closing))
      .catch(report);
  };
  /** The strip of this pane is the one that the person drags. */
  const item = useValue(dragged);
  const lifted = item?.kind === 'pane' && item.id === props.id;
  // A path of the Files tab over the pane: a drop pastes it, and the pane takes the keyboard.
  const [pathOver, setPathOver] = useState(false);
  const latest = useLatest({ id: props.id, focus });
  const target = useCallback(
    (element: HTMLElement | null) => {
      if (!element) return;
      const dropPath = (path: string): void => {
        setPathOver(false);
        input
          .insertPaths(latest.current.id, [path])
          .then(() => {
            latest.current.focus();
            element
              .querySelector<HTMLTextAreaElement>('.pane-input')
              ?.focus({ preventScroll: true });
          })
          .catch(report);
      };
      dropTarget(element, {
        accepts: (item) => item.kind === 'path',
        enter: () => setPathOver(true),
        leave: () => setPathOver(false),
        drop: (item) => {
          if (item.kind === 'path') dropPath(item.path);
        },
      });
    },
    [input, report, latest],
  );
  // The details of the pane: a popover under the button i.
  const detailsId = `pane-details-${props.id}`;
  const [detailsOpen, setDetailsOpen] = useState(false);
  const details = pane ? paneDetails(pane, run) : [];
  const copyDetail = (label: string, value: string): void => {
    writeClipboard(Promise.resolve(value))
      .then(() => toasts.show({ title: `${label} copied`, tone: 'ok' }))
      .catch(report);
  };

  // A program asked to write the clipboard with OSC 52, and the core waits for the answer.
  const [remember, setRemember] = useState(false);
  const answerClipboard = (allow: boolean): void => {
    const answer = transport.call('pane.clipboard_allow', {
      pane: props.id,
      allow,
      remember,
      // A phone writes its own clipboard in the tap.
      ...(phone ? { return_text: true } : {}),
    });
    setRemember(false);
    if (phone && allow) {
      writeClipboard(answer.then((result) => result.text ?? '')).catch(report);
    } else {
      answer.catch(report);
    }
  };

  /** Sends the files of the picker of iOS to the pane: the core pastes their paths. */
  const attachFiles = async (files: FileList | null): Promise<void> => {
    if (!files || !upload) return;
    const blobs: { hash: string; name: string }[] = [];
    for (const file of Array.from(files)) {
      blobs.push({ hash: await upload(file), name: file.name });
    }
    if (blobs.length === 0) return;
    await transport.call('pane.drop_blobs', {
      pane: props.id,
      input_id: input.nextId(),
      blobs,
    });
  };

  /** The menu ⋯ of a pane on a phone. */
  const openMenu = (x: number, y: number): void => {
    const split = (direction: 'right' | 'down') =>
      call('pane.split', { pane: props.id, direction });
    view.pressMenu.set({
      x,
      y,
      title: pane?.title || 'pane',
      items: [
        { label: 'Split right', run: () => split('right') },
        { label: 'Split down', run: () => split('down') },
        { label: zoomed ? 'Show all panes' : 'Zoom', run: toggleZoom },
        { label: floating ? 'Dock' : 'Float', run: toggleFloat },
        {
          label: 'Fit to this phone',
          run: fit,
        },
        { label: 'Attach a file', run: () => attach.current?.click() },
        {
          label: 'Paste',
          run: () => void readClipboard().then((text) => input.paste(props.id, text), report),
        },
        { label: 'Close', run: close, danger: true },
      ],
    });
  };
  const badge = run ? badgeOf(run) : undefined;
  const title = stripTitle(pane?.title ?? '', run?.harness);
  const unconfirmed = pane?.input_unconfirmed?.length ?? 0;
  const dropped = pane?.clipboard_dropped;
  const request = pane?.clipboard_request;
  return (
    <section
      className={cn('pane', {
        focused,
        restoring: pane?.restoring === true,
        zoomed,
        lifted,
        'path-over': pathOver,
      })}
      data-pane={props.id}
      aria-label={pane?.title || 'pane'}
      onPointerDown={focus}
      ref={target}
    >
      <header
        className="pane-title"
        onPointerDown={(event) => {
          // A floating pane moves by its strip in the tab, and a button of the strip only clicks.
          // The strip of the single view takes a swipe, not a drag.
          if (props.single || floating || zoomed || (event.target as Element).closest('button'))
            return;
          pressToDrag(event.nativeEvent, { kind: 'pane', id: props.id });
        }}
        onTouchStart={(event) => {
          if (props.single) swipeX.current = event.touches[0]?.clientX ?? null;
        }}
        onTouchEnd={(event) => {
          const endX = event.changedTouches[0]?.clientX;
          const startX = swipeX.current;
          if (startX !== null && endX !== undefined) {
            if (endX - startX <= -SWIPE) show(1);
            else if (endX - startX >= SWIPE) show(-1);
          }
          swipeX.current = null;
        }}
      >
        {siblings.length > 1 ? (
          <span
            className="pane-dots"
            role="img"
            aria-label={`Pane ${place + 1} of ${siblings.length}`}
          >
            {siblings.map((id) => (
              <span key={id} className={cn('pane-dot', { current: id === props.id })} />
            ))}
          </span>
        ) : null}
        {badge ? <span className="pane-edge" data-code={badge.code} aria-hidden="true" /> : null}
        {handles ? <span className="pane-handle">{pane?.handle}</span> : null}
        {run && badge ? (
          <>
            {['WAIT', 'ERR'].includes(badge.code) ? (
              <span className="pane-state" data-code={badge.code}>
                {badge.code}
              </span>
            ) : null}
            <HarnessBadge run={run} className="pane-harness" />
            {badge.unvalidated ? (
              <span
                className="pane-unvalidated"
                role="img"
                aria-label="Version not validated"
                title={`This version of ${harnessName(run.harness)} is not validated: answers from neohar may not reach it.`}
              >
                ?
              </span>
            ) : null}
            {run.yolo ? (
              <Badge tone="muted" className="pane-yolo" title="No approval dialogs">
                yolo
              </Badge>
            ) : null}
          </>
        ) : null}
        {title ? <span className="pane-name">{title}</span> : null}
        {unconfirmed > 0 ? (
          <span
            className="pane-unconfirmed"
            role="status"
            title={`Input that may not have arrived: ${pane?.input_unconfirmed?.join(', ')}`}
          >
            ? {pane?.input_unconfirmed?.length}
          </span>
        ) : null}
        {dropped ? (
          <span
            className="pane-clipboard-dropped"
            role="status"
            title={`A program wrote ${dropped.bytes} bytes to the clipboard, and the client that typed last is not connected. The text was dropped.`}
          >
            clipboard dropped
          </span>
        ) : null}
        {otherSize ? (
          <button
            type="button"
            className="pane-size"
            data-action="fit"
            title="Another window has the size of this terminal. Fit it to this window."
            onClick={fit}
          >
            Fit
          </button>
        ) : null}
        <span className="pane-spacer" />
        <button
          type="button"
          className="pane-button"
          data-action="details"
          popoverTarget={detailsId}
          aria-expanded={detailsOpen}
          aria-label="Pane details"
          title="Pane details"
        >
          <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
            <circle cx="8" cy="8" r="6.25" fill="none" stroke="currentColor" strokeWidth="1.3" />
            <path d="M8 7.25V11.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
            <circle cx="8" cy="4.9" r="0.95" fill="currentColor" />
          </svg>
        </button>
        {phone ? (
          <>
            <button
              type="button"
              className="pane-button"
              data-action="menu"
              aria-label="Pane actions"
              aria-haspopup="menu"
              onClick={(event) => {
                const box = event.currentTarget.getBoundingClientRect();
                openMenu(box.right - 220, box.bottom);
              }}
            >
              <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
                <circle cx="3" cy="8" r="1.4" fill="currentColor" />
                <circle cx="8" cy="8" r="1.4" fill="currentColor" />
                <circle cx="13" cy="8" r="1.4" fill="currentColor" />
              </svg>
            </button>
            <input
              ref={attach}
              className="pane-attach"
              type="file"
              multiple
              hidden
              aria-label="Files to attach"
              onChange={(event) => {
                const files = event.currentTarget.files;
                attachFiles(files).catch(report);
                event.currentTarget.value = '';
              }}
            />
          </>
        ) : null}
        <button
          type="button"
          className="pane-button"
          data-action="float"
          aria-pressed={floating}
          aria-label={floating ? 'Dock pane' : 'Float pane'}
          title={floating ? 'Dock pane' : 'Float pane'}
          onClick={toggleFloat}
        >
          <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
            <rect x="2" y="2" width="9" height="9" fill="none" stroke="currentColor" />
            <rect x="6" y="6" width="8" height="8" fill="var(--bg-raised)" stroke="currentColor" />
          </svg>
        </button>
        <button
          type="button"
          className="pane-button"
          data-action="zoom"
          aria-pressed={zoomed}
          aria-label={zoomed ? 'Show all panes' : 'Zoom pane'}
          title={zoomed ? 'Show all panes' : 'Zoom pane'}
          onClick={toggleZoom}
        >
          <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
            <path
              d="M2 6V2h4M10 2h4v4M14 10v4h-4M6 14H2v-4"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.4"
            />
          </svg>
        </button>
        <button
          type="button"
          className="pane-button pane-close"
          data-action="close"
          aria-label="Close pane"
          title="Close pane"
          onClick={close}
        >
          <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
            <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" />
          </svg>
        </button>
      </header>
      <Popover
        id={detailsId}
        className="pane-details"
        role="dialog"
        aria-label={`Details of ${pane?.title || 'the pane'}`}
        onToggle={setDetailsOpen}
      >
        <div className="pane-details-head">
          {run ? (
            <>
              <HarnessBadge run={run} />
              {run.yolo ? (
                <Badge tone="muted" title="No approval dialogs">
                  yolo
                </Badge>
              ) : null}
            </>
          ) : (
            <span className="pane-details-title">{detailsTitle(run)}</span>
          )}
        </div>
        <dl className="pane-details-list">
          {details.map((row) => (
            <div key={row.key} className="pane-detail" data-detail={row.key}>
              <dt>{row.label}</dt>
              <dd className={row.data ? 'pane-detail-data' : undefined}>
                <span className="pane-detail-value">
                  {/* A path breaks after a slash, where a person reads it. */}
                  {row.value.split(/(?<=\/)/).map((part, at) => (
                    <Fragment key={at}>
                      {part}
                      <wbr />
                    </Fragment>
                  ))}
                </span>
                {row.copy ? (
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    className="pane-detail-copy"
                    aria-label={`Copy the ${row.label.toLowerCase()}`}
                    title={`Copy the ${row.label.toLowerCase()}`}
                    onClick={() => copyDetail(row.label, row.value)}
                  >
                    <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
                      <rect x="5.5" y="5.5" width="8" height="8" rx="1.5" />
                      <path d="M10.5 3.5v-.25A1.25 1.25 0 0 0 9.25 2h-6A1.25 1.25 0 0 0 2 3.25v6A1.25 1.25 0 0 0 3.25 10.5h.25" />
                    </svg>
                  </Button>
                ) : null}
              </dd>
            </div>
          ))}
        </dl>
      </Popover>
      {request ? (
        request.allowed ? (
          // A remembered Allow on a phone: the write waits for this tap.
          <div className="pane-held pane-clipboard" role="alert">
            <span>
              A program copied {request.bytes} bytes:{' '}
              <q className="clipboard-preview">{request.preview}</q>
            </span>
            <button
              type="button"
              data-action="clipboard-allow"
              onClick={() => answerClipboard(true)}
            >
              Copy
            </button>
            <button
              type="button"
              data-action="clipboard-deny"
              onClick={() => answerClipboard(false)}
            >
              Dismiss
            </button>
          </div>
        ) : (
          <div className="pane-held pane-clipboard" role="alert">
            <span>
              A program wants to write {request.bytes} bytes to the clipboard:{' '}
              <q className="clipboard-preview">{request.preview}</q>
            </span>
            <label className="clipboard-remember">
              <input
                type="checkbox"
                checked={remember}
                onChange={(e) => setRemember(e.currentTarget.checked)}
              />{' '}
              Remember for this pane
            </label>
            <button
              type="button"
              data-action="clipboard-allow"
              onClick={() => answerClipboard(true)}
            >
              Allow
            </button>
            <button
              type="button"
              data-action="clipboard-deny"
              onClick={() => answerClipboard(false)}
            >
              Deny
            </button>
          </div>
        )
      ) : null}
      <div className="pane-body">
        <Surface id={props.id} focused={focused} />
      </div>
      {exited ? (
        <div className="pane-exited" role="status">
          <span>
            {exited.code === null || exited.code === undefined
              ? 'The program ended.'
              : `The program ended with code ${exited.code}.`}
          </span>
          <button type="button" data-action="restart" onClick={restart}>
            Restart
          </button>
        </div>
      ) : null}
    </section>
  );
});
