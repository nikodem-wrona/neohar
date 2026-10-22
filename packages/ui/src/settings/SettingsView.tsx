// The settings window. It shows the `settings` projection: each section of the
// configuration with its settings, their values and where each value comes from (the defaults of
// the core, the file, or this window). An edit calls `config.set`, and the projection brings the
// new value back. The window is never a second source of truth: it keeps no value of its own.

import { RpcError, type SettingEntry, type Transport } from '@neohar/transport';
import { createSignal, For, type JSX, Match, onCleanup, Show, Switch } from 'solid-js';
import { createProjectionStore } from '../stores';
import { fieldText, humanize, parseEdit } from './format';
import { PhoneSection } from './PhoneSection';

/** The section that the fragment of the address names, for example `#phone`. */
function sectionOfHash(): string | null {
  const hash = window.location.hash.slice(1);
  return hash === '' ? null : hash;
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
  const label = () => humanize(props.name);
  return (
    <div class="setting" data-setting={props.path}>
      <label class="setting-label" for={`setting-${props.path}`}>
        {label()}
      </label>
      <Switch>
        <Match when={typeof props.entry.value === 'boolean'}>
          <input
            id={`setting-${props.path}`}
            type="checkbox"
            checked={props.entry.value === true}
            onChange={(event) => props.onToggle(event.currentTarget.checked)}
          />
        </Match>
        <Match when={typeof props.entry.value === 'number'}>
          <input
            id={`setting-${props.path}`}
            class="setting-input"
            type="number"
            value={fieldText(props.entry.value)}
            onChange={(event) => props.onEdit(event.currentTarget.value)}
          />
        </Match>
        <Match when={true}>
          <input
            id={`setting-${props.path}`}
            class="setting-input"
            type="text"
            spellcheck={false}
            value={fieldText(props.entry.value)}
            onChange={(event) => props.onEdit(event.currentTarget.value)}
          />
        </Match>
      </Switch>
      <span class="setting-source" data-source={props.entry.source}>
        {props.entry.source}
      </span>
      <Show when={props.entry.source !== 'default'}>
        <button type="button" class="setting-reset" onClick={props.onReset}>
          Reset
        </button>
      </Show>
      <Show when={props.error}>
        <p class="setting-error" role="alert">
          {props.error}
        </p>
      </Show>
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
  section?: () => string | null;
  onSection?: (name: string) => void;
  /** More content of a section, before its settings: This phone on a phone. */
  extra?: (section: string) => JSX.Element | undefined;
}

export function SettingsView(props: SettingsViewProps) {
  const store = createProjectionStore(props.transport, 'settings');
  onCleanup(() => store.close());
  // The host opens the window at a section with a fragment, and changes the fragment of a window
  // that is open.
  const [fromHash, setFromHash] = createSignal<string | null>(sectionOfHash());
  if (!props.section) {
    const onHash = (): void => {
      setFromHash(sectionOfHash());
    };
    window.addEventListener('hashchange', onHash);
    onCleanup(() => window.removeEventListener('hashchange', onHash));
  }
  const chosen = () => (props.section ? props.section() : fromHash());
  const setChosen = (name: string): void => {
    if (props.onSection) {
      props.onSection(name);
      return;
    }
    setFromHash(name);
    // The fragment follows, so that the host can open this section again.
    history.replaceState(null, '', `#${name}`);
  };
  const active = () => {
    const name = chosen();
    return name !== null && store.state.order.includes(name)
      ? name
      : (store.state.order[0] ?? null);
  };
  const [errors, setErrors] = createSignal<Record<string, string>>({});

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
    <div class="settings">
      <nav class="settings-nav" aria-label="Sections">
        <For each={store.state.order}>
          {(name) => (
            <button
              type="button"
              class="settings-section-button"
              classList={{ selected: name === active() }}
              aria-current={name === active() ? 'page' : undefined}
              data-section={name}
              onClick={() => setChosen(name)}
            >
              {humanize(name)}
            </button>
          )}
        </For>
      </nav>
      <main class="settings-main">
        <Show when={store.state.status === 'loading'}>
          <p class="settings-note">Waiting for the core.</p>
        </Show>
        <Show when={active()} keyed>
          {(name) => (
            <section aria-label={humanize(name)}>
              <h1 class="settings-title">{humanize(name)}</h1>
              {props.extra?.(name)}
              <Show when={name === 'phone'}>
                <PhoneSection transport={props.transport} />
              </Show>
              <For each={Object.keys(store.state.byKey[name]?.entries ?? {})}>
                {(key) => (
                  <Show when={store.state.byKey[name]?.entries[key]}>
                    {(entry) => {
                      const path = `${name}.${key}`;
                      return (
                        <Row
                          path={path}
                          name={key}
                          entry={entry()}
                          error={errors()[path]}
                          onEdit={(text) => {
                            const edit = parseEdit(text, entry().value);
                            if (edit.ok) send(path, edit.value);
                            else setError(path, edit.error);
                          }}
                          onToggle={(value) => send(path, value)}
                          onReset={() => send(path, null)}
                        />
                      );
                    }}
                  </Show>
                )}
              </For>
            </section>
          )}
        </Show>
      </main>
    </div>
  );
}
