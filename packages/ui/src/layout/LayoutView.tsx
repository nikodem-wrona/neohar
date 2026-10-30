// The layout of the tab that the window shows: splits as flex boxes with dividers, panes as boxes
// with a title strip, and the floating panes over them. A zoomed pane fills the tab. The surface
// of a pane goes into its body. A window with the single view of a phone draws the one
// pane that the core names in `visible_panes`.

import type { LayoutNode, TabValue } from '@neohar/transport';
import { useRef } from 'react';
import { useApp } from '../app/context';
import { useSelect } from '../lib/hooks';
import { cn } from '../lib/utils';
import { Floating } from './Floating';
import { PaneBox } from './PaneBox';
import { SplitView } from './SplitView';
import { asSplit } from './tree';

function Node(props: { tab: TabValue; node: LayoutNode; path: number[] }) {
  const split = asSplit(props.node);
  if (split) {
    return (
      <SplitView
        tab={props.tab}
        split={split}
        path={props.path}
        child={(node, path) => <Node tab={props.tab} node={node} path={path} />}
      />
    );
  }
  return props.node.type === 'pane' ? <PaneBox id={props.node.pane} tab={props.tab} /> : null;
}

export function LayoutView(props: { tab: TabValue }) {
  const { stores, windowId } = useApp();
  const area = useRef<HTMLDivElement>(null);
  /** The pane of the single view: the core names it. */
  const single = useSelect(stores.windows, (state) => {
    const win = state.byKey[windowId];
    return win?.single ? win.visible_panes[0] : undefined;
  });
  const body = () => {
    if (single !== undefined) return <PaneBox key={single} id={single} tab={props.tab} single />;
    const zoomed = props.tab.zoomed;
    if (zoomed) return <PaneBox key={zoomed} id={zoomed} tab={props.tab} />;
    return (
      <>
        <Node tab={props.tab} node={props.tab.layout} path={[]} />
        {(props.tab.floating ?? []).map((item) => (
          <Floating key={item.pane} tab={props.tab} item={item} area={area} />
        ))}
      </>
    );
  };
  return (
    <div
      className={cn('layout', { single: single !== undefined })}
      data-tab={props.tab.id}
      ref={area}
    >
      {body()}
    </div>
  );
}
