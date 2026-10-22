// The layout of the tab that the window shows: splits as flex boxes with dividers, panes as boxes
// with a title strip, and the floating panes over them. A zoomed pane fills the tab. The surface
// of a pane goes into its body. A window with the single view of a phone draws the one
// pane that the core names in `visible_panes`.

import type { LayoutNode, TabValue } from '@neohar/transport';
import { For, Match, Show, Switch } from 'solid-js';
import { useApp } from '../app/context';
import { Floating } from './Floating';
import { PaneBox } from './PaneBox';
import { SplitView } from './SplitView';
import { asSplit } from './tree';

function Node(props: { tab: TabValue; node: LayoutNode; path: number[] }) {
  return (
    <Switch>
      <Match when={asSplit(props.node)}>
        {(split) => (
          <SplitView
            tab={props.tab}
            split={split()}
            path={props.path}
            child={(node, path) => <Node tab={props.tab} node={node} path={path} />}
          />
        )}
      </Match>
      <Match when={props.node.type === 'pane' ? props.node : undefined}>
        {(leaf) => <PaneBox id={leaf().pane} tab={props.tab} />}
      </Match>
    </Switch>
  );
}

export function LayoutView(props: { tab: TabValue }) {
  const { stores, windowId } = useApp();
  let area: HTMLDivElement | undefined;
  /** The pane of the single view: the core names it. */
  const single = () => {
    const win = stores.windows.state.byKey[windowId];
    return win?.single ? win.visible_panes[0] : undefined;
  };
  return (
    <div
      class="layout"
      classList={{ single: single() !== undefined }}
      data-tab={props.tab.id}
      ref={(el) => (area = el)}
    >
      <Show
        when={single()}
        keyed
        fallback={
          <Show
            when={props.tab.zoomed}
            fallback={
              <>
                <Node tab={props.tab} node={props.tab.layout} path={[]} />
                <For each={props.tab.floating}>
                  {(item) => <Floating tab={props.tab} item={item} area={() => area} />}
                </For>
              </>
            }
          >
            {(zoomed) => <PaneBox id={zoomed()} tab={props.tab} />}
          </Show>
        }
      >
        {(pane) => <PaneBox id={pane} tab={props.tab} single />}
      </Show>
    </div>
  );
}
