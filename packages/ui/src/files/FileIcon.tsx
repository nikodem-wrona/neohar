// The icon of an entry of the Files tab, in the colours of the Material Icon Theme. The core names
// the icon (`icon` of `fs.list` and `fs.read`), and this component only draws its art. The art is a
// chunk of its own that loads with the first icon, so that the first paint of the phone does not
// wait for it. Until it is there, an icon is an empty square of the same size. A name that the art
// does not have draws the icon of any file or folder.

import { createMemo, createSignal, For, Show } from 'solid-js';
import type { IconArt, IconPath } from './icon-art';

type Art = typeof import('./icon-art');

const [art, setArt] = createSignal<Art | null>(null);
let loading = false;

function load(): void {
  if (loading) return;
  loading = true;
  import('./icon-art').then(
    (module) => setArt(() => module),
    // The next icon asks again.
    () => (loading = false),
  );
}

/** The art to draw: the icon of a file, or the closed or open icon of a folder. */
function drawn(module: Art, icon: string, folder: boolean, open: boolean): IconArt {
  if (!folder) return module.FILE_ART[icon] ?? module.FILE_ART['file']!;
  const shown = module.FOLDER_ART[icon] ?? module.FOLDER_ART['folder']!;
  if (typeof shown[0] !== 'string') {
    const [closed, opened] = shown as readonly [IconArt, IconArt];
    return open ? opened : closed;
  }
  const [color, motive] = shown as readonly [string, readonly IconPath[]];
  return ['0 0 16 16', [[color, open ? module.OPEN_FOLDER : module.CLOSED_FOLDER], ...motive]];
}

export function FileIcon(props: { icon: string; folder?: boolean; open?: boolean }) {
  load();
  const shown = createMemo(() => {
    const module = art();
    return module ? drawn(module, props.icon, props.folder ?? false, props.open ?? false) : null;
  });
  return (
    <svg
      class="file-icon"
      data-icon={props.icon}
      width="16"
      height="16"
      viewBox={shown()?.[0]}
      aria-hidden="true"
    >
      <Show when={shown()}>
        {(now) => (
          <For each={now()[1]}>
            {([fill, d, evenOdd]) => (
              <path fill={fill} d={d} fill-rule={evenOdd ? 'evenodd' : undefined} />
            )}
          </For>
        )}
      </Show>
    </svg>
  );
}
