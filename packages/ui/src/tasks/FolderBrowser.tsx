// The folder browser of the task dialog. The page shows it when
// `window.pick_folder` answers `unsupported`: a window of another machine, or a core with no
// folder panel. It calls `fs.dirs`. The core sends the folders in the order shown here (the
// favorites first, then by name), so the page neither sorts nor filters. A click on a folder
// opens it, the first row opens the parent, and "Choose" takes the folder that the browser shows.

import { RpcError, type FsDirsResult } from '@neohar/transport';
import { createSignal, For, onMount, Show } from 'solid-js';
import { useApp } from '../app/context';
import { truncatedLabel } from './model';

export function FolderBrowser(props: {
  /** The folder to open first. Without it, or when it is not a folder, the home folder. */
  start: string;
  onChoose: (path: string) => void;
  onClose: () => void;
}) {
  const { transport, report } = useApp();
  const [listing, setListing] = createSignal<FsDirsResult | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  // An answer that comes after a newer click is dropped.
  let latest = 0;

  const open = async (path?: string): Promise<boolean> => {
    const mine = ++latest;
    setBusy(true);
    try {
      const result = await transport.call('fs.dirs', path === undefined ? {} : { path });
      if (mine !== latest) return true;
      setListing(result);
      setError(null);
      return true;
    } catch (failure: unknown) {
      if (mine === latest) {
        setError(failure instanceof RpcError ? failure.message : 'The folders did not load.');
      }
      return false;
    } finally {
      if (mine === latest) setBusy(false);
    }
  };

  onMount(() => {
    // A start that is not a folder gives the home folder.
    (props.start === '' ? open() : open(props.start).then((ok) => ok || open())).catch(report);
  });

  return (
    <div
      class="folder-browser"
      role="group"
      aria-label="Folders"
      aria-busy={busy()}
      // WebKit gives no focus to a button that the person clicks. The browser takes it, so that
      // Escape closes the browser and not the dialog.
      tabIndex={-1}
      onKeyDown={(event) => {
        // Escape closes the browser. A second Escape closes the dialog.
        if (event.key === 'Escape') {
          event.preventDefault();
          event.stopPropagation();
          props.onClose();
        }
      }}
    >
      <Show when={listing()}>
        {(shown) => (
          <>
            <div class="folder-path" title={shown().path}>
              {shown().path}
            </div>
            <ul class="folder-list">
              <Show when={shown().parent}>
                {(parent) => (
                  <li>
                    <button
                      type="button"
                      class="folder-row folder-parent"
                      aria-label="Parent folder"
                      title={parent()}
                      onClick={() => void open(parent()).catch(report)}
                    >
                      <span class="folder-name">..</span>
                    </button>
                  </li>
                )}
              </Show>
              <For each={shown().entries}>
                {(entry) => (
                  <li>
                    <button
                      type="button"
                      class="folder-row"
                      title={entry.path}
                      onClick={() => void open(entry.path).catch(report)}
                    >
                      <span class="folder-name">{entry.name}</span>
                      <Show when={entry.repo}>
                        <span class="folder-repo" title="A git repository">
                          repo
                        </span>
                      </Show>
                    </button>
                  </li>
                )}
              </For>
            </ul>
            <Show when={shown().entries.length === 0}>
              <p class="task-hint">No folders here.</p>
            </Show>
            <Show when={shown().truncated}>
              <p class="task-hint">{truncatedLabel(shown().entries.length)}</p>
            </Show>
          </>
        )}
      </Show>
      <Show when={error()}>
        <p class="task-error" role="alert">
          {error()}
        </p>
      </Show>
      <div class="folder-buttons">
        <button
          type="button"
          class="primary"
          disabled={listing() === null}
          onClick={() => {
            const path = listing()?.path;
            if (path) props.onChoose(path);
          }}
        >
          Choose
        </button>
        <button type="button" onClick={props.onClose}>
          Close
        </button>
      </div>
    </div>
  );
}
