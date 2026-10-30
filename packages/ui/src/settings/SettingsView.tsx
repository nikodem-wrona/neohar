// The settings window. It shows the `settings` projection: each section of the
// configuration with its settings, their values and where each value comes from (the defaults of
// the core, the file, or this window). An edit calls `config.set`, and the projection brings the
// new value back. The window is never a second source of truth: it keeps no value of its own.

import { RpcError, type SettingEntry, type Transport } from '@neohar/transport';
import { type ReactNode, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useLatest, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { useProjectionStore } from '../stores';
import { fieldText, humanize, parseEdit } from './format';
import { PhoneSection } from './PhoneSection';

/** The section that the fragment of the address names, for example `#phone`. */
function sectionOfHash(): string | null {
  const hash = window.location.hash.slice(1);
  return hash === '' ? null : hash;
}

/**
 * A field of a number or of a text. The person edits it freely: it sends the text on the native
 * `change` (on blur or Enter), one time for each value, not for each key. A new value from the
 * core goes into the field.
 */
function TextField(props: {
  id: string;
  type: 'number' | 'text';
  text: string;
  onEdit: (text: string) => void;
}) {
  const field = useRef<HTMLInputElement>(null);
  const onEdit = useLatest(props.onEdit);
  useLayoutEffect(() => {
    if (field.current) field.current.value = props.text;
  }, [props.text]);
  useEffect(() => {
    const element = field.current;
    if (!element) return;
    const change = (): void => onEdit.current(element.value);
    element.addEventListener('change', change);
    return () => element.removeEventListener('change', change);
  }, [onEdit]);
  return (
    <input
      id={props.id}
      className="setting-input"
      type={props.type}
      {...(props.type === 'text' ? { spellCheck: false } : {})}
      ref={field}
    />
  );
}

/**
 * A check box. It shows the value of the core: a click changes the box at once, and the box
 * follows the value that the core brings back.
 */
function CheckField(props: { id: string; checked: boolean; onToggle: (value: boolean) => void }) {
  const box = useRef<HTMLInputElement>(null);
  useLayoutEffect(() => {
    if (box.current) box.current.checked = props.checked;
  }, [props.checked]);
  return (
    <input
      id={props.id}
      type="checkbox"
      ref={box}
      onChange={(event) => props.onToggle(event.currentTarget.checked)}
    />
  );
}

function Row(props: {
  path: string;
  name: string;
  entry: SettingEntry;
  error: string | undefined;
  onEdit: (text: string) => void;
  onToggle: (value: boolean) => void;
  onReset: () => void;
}) {
  const label = humanize(props.name);
  const id = `setting-${props.path}`;
  const kind = typeof props.entry.value;
  return (
    <div className="setting" data-setting={props.path}>
      <label className="setting-label" htmlFor={id}>
        {label}
      </label>
      {kind === 'boolean' ? (
        <CheckField
          key="boolean"
          id={id}
          checked={props.entry.value === true}
          onToggle={props.onToggle}
        />
      ) : (
        <TextField
          key={kind === 'number' ? 'number' : 'text'}
          id={id}
          type={kind === 'number' ? 'number' : 'text'}
          text={fieldText(props.entry.value)}
          onEdit={props.onEdit}
        />
      )}
      <span className="setting-source" data-source={props.entry.source}>
        {props.entry.source}
      </span>
      {props.entry.source !== 'default' ? (
        <button type="button" className="setting-reset" onClick={props.onReset}>
          Reset
        </button>
      ) : null}
      {props.error ? (
        <p className="setting-error" role="alert">
          {props.error}
        </p>
      ) : null}
    </div>
  );
}

export interface SettingsViewProps {
  transport: Transport;
  /**
   * The section that shows, and what a choice of a section does. Without them, the fragment of
   * the address is the section, as the host of the app opens the window at a section. The desk
   * on a phone has addresses of its own.
   */
  section?: string | null | undefined;
  onSection?: (name: string) => void;
  /** More content of a section, before its settings: This phone on a phone. */
  extra?: (section: string) => ReactNode;
}

export function SettingsView(props: SettingsViewProps) {
  const store = useProjectionStore(props.transport, 'settings');
  const state = useValue(store);
  // The host opens the window at a section with a fragment, and changes the fragment of a window
  // that is open.
  const [fromHash, setFromHash] = useState<string | null>(sectionOfHash);
  const controlled = props.section !== undefined;
  useEffect(() => {
    if (controlled) return;
    const onHash = (): void => {
      setFromHash(sectionOfHash());
    };
    window.addEventListener('hashchange', onHash);
    return () => window.removeEventListener('hashchange', onHash);
  }, [controlled]);
  const chosen = controlled ? (props.section ?? null) : fromHash;
  const setChosen = (name: string): void => {
    if (props.onSection) {
      props.onSection(name);
      return;
    }
    setFromHash(name);
    // The fragment follows, so that the host can open this section again.
    history.replaceState(null, '', `#${name}`);
  };
  const active =
    chosen !== null && state.order.includes(chosen) ? chosen : (state.order[0] ?? null);
  const [errors, setErrors] = useState<Record<string, string>>({});

  const setError = (path: string, message: string | undefined): void => {
    setErrors((all) => {
      const next = { ...all };
      if (message === undefined) delete next[path];
      else next[path] = message;
      return next;
    });
  };

  /** Sends a value to the core. The core checks it, and an error shows under the setting. */
  const send = (path: string, value: unknown): void => {
    props.transport
      .call('config.set', { key: path, value })
      .then(() => setError(path, undefined))
      .catch((failure: unknown) =>
        setError(path, failure instanceof RpcError ? failure.message : 'The core did not answer.'),
      );
  };

  return (
    <div className="settings">
      <nav className="settings-nav" aria-label="Sections">
        {state.order.map((name) => (
          <button
            key={name}
            type="button"
            className={cn('settings-section-button', { selected: name === active })}
            aria-current={name === active ? 'page' : undefined}
            data-section={name}
            onClick={() => setChosen(name)}
          >
            {humanize(name)}
          </button>
        ))}
      </nav>
      <main className="settings-main">
        {state.status === 'loading' ? <p className="settings-note">Waiting for the core.</p> : null}
        {active ? (
          <section key={active} aria-label={humanize(active)}>
            <h1 className="settings-title">{humanize(active)}</h1>
            {props.extra?.(active)}
            {active === 'phone' ? <PhoneSection transport={props.transport} /> : null}
            {Object.keys(state.byKey[active]?.entries ?? {}).map((key) => {
              const entry = state.byKey[active]?.entries[key];
              if (!entry) return null;
              const path = `${active}.${key}`;
              return (
                <Row
                  key={key}
                  path={path}
                  name={key}
                  entry={entry}
                  error={errors[path]}
                  onEdit={(text) => {
                    const edit = parseEdit(text, entry.value);
                    if (edit.ok) send(path, edit.value);
                    else setError(path, edit.error);
                  }}
                  onToggle={(value) => send(path, value)}
                  onReset={() => send(path, null)}
                />
              );
            })}
          </section>
        ) : null}
      </main>
    </div>
  );
}
