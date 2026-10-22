// Small helpers for the layout tree that the `tree` projection carries. The UI draws the tree
// with CSS and does not compute a layout.

import type { LayoutNode, PaneId } from '@neohar/transport';

type Split = Extract<LayoutNode, { type: 'split' }>;

/** The node as a split, or `undefined` for a pane. */
export function asSplit(node: LayoutNode): Split | undefined {
  return node.type === 'split' ? node : undefined;
}

/** The ids of the panes below a node, from the top left. */
export function paneIds(node: LayoutNode): PaneId[] {
  if (node.type === 'pane') return [node.pane];
  return node.children.flatMap(paneIds);
}

/**
 * The panes of a tab in the order of its tree, and then its floating panes: the dots and the
 * swipe of the single view of a phone go through them.
 */
export function tabPanes(tab: {
  layout: LayoutNode;
  floating?: readonly { pane: PaneId }[];
}): PaneId[] {
  return [...paneIds(tab.layout), ...(tab.floating ?? []).map((item) => item.pane)];
}

/** The CSS `flex` value for a child of a split. The share is the grow factor. */
export function flexOf(ratio: number | undefined): string {
  const share = ratio !== undefined && ratio > 0 ? ratio : 1;
  return `${share} 1 0%`;
}
