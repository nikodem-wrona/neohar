// The frame that every card shares, as the Card of shadcn/ui (ui.shadcn.com) lays it out: the
// head with the title on the left and the risk, the state and the handle on the right, the body,
// the line for what the person must know, and the error of the last click. A card that is not
// open is one row: its title and a badge that says what became of it. A toast says that an answer
// of this window arrived (`cards/deliveries.ts`). Cards never overlay a pane; the drawer lays them
// out.

import type { InteractionValue } from '@neohar/transport';
import type { KeyboardEvent, ReactNode, Ref } from 'react';
import { Badge } from '../components/ui/badge';
import {
  Card,
  CardAction,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from '../components/ui/card';
import { cn } from '../lib/utils';
import { stateBadge, type Status, statusOf } from './model';

/**
 * The shape of a card of the Inbox, on the Card of shadcn/ui: the gaps and the padding of the
 * chrome, and a ring around the card itself for the focus of ⌘⇧A. A control in the card shows its
 * own focus, so the card has no ring then.
 */
export const CARD =
  'card min-w-0 gap-(--s3) rounded-lg p-0 px-(--s4) pt-(--s3) pb-(--s4) shadow-none focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-foreground';

/** The parts of a card: no padding of their own, the card has it. */
export const PART = 'px-0';

export function CardFrame(props: {
  interaction: InteractionValue;
  harness: string;
  error: string | null;
  /** The title while the card is open, when it is not the title of the interaction. */
  title?: string | undefined;
  children: ReactNode;
  /** The answers of the card, under its body while it is open. */
  footer?: ReactNode;
  onKeyDown?: ((event: KeyboardEvent<HTMLElement>) => void) | undefined;
  ref?: Ref<HTMLElement> | undefined;
}) {
  const { interaction } = props;
  const open = interaction.state === 'open';
  const status: Status | null = statusOf(interaction, props.harness);
  const badge: Status | null = stateBadge(interaction, props.harness);
  // A card that waits for a person has an amber border and tint, and a red one at a high risk. A
  // card whose answer may not have arrived stays amber.
  const waits = open || interaction.state === 'delivery_unknown';
  const danger = open && interaction.kind === 'approval' && interaction.risk === 'high';
  return (
    <Card
      asChild
      className={cn(
        CARD,
        `card-${interaction.kind}`,
        waits && 'border-caution bg-(--tint-caution)',
        danger && 'border-destructive bg-(--tint-danger)',
        // A card that ended is one row: its title on one line, and the badge of its end. It keeps
        // the contrast of its text: a dashed border and a dim title, not a lower opacity (WCAG
        // 1.4.3).
        !open && 'gap-(--s2) border-dashed py-(--s2)',
      )}
    >
      <article
        data-interaction={interaction.id}
        data-state={interaction.state}
        tabIndex={0}
        aria-label={`${interaction.handle}: ${interaction.title}`}
        onKeyDown={props.onKeyDown}
        ref={props.ref}
      >
        {/* The head, as in shadcn/ui: the title on the left, and at the right the risk, the badge
            of the state and the handle. */}
        <CardHeader className={cn(PART, 'flex gap-(--s3)', !open && 'items-center')}>
          <CardTitle
            role="heading"
            aria-level={3}
            className={cn(
              'card-title min-w-0 flex-1 leading-5 [overflow-wrap:anywhere]',
              !open && 'truncate [font:var(--t-body)] text-muted-foreground',
            )}
          >
            {open ? (props.title ?? interaction.title) : interaction.title}
          </CardTitle>
          <CardAction
            className={cn('flex min-h-5 flex-none items-center gap-(--s2)', !open && 'self-center')}
          >
            {open && interaction.kind === 'approval' ? (
              <Badge
                className={cn('uppercase', interaction.risk === 'medium' && 'text-caution')}
                data-risk={interaction.risk}
                variant={interaction.risk === 'high' ? 'error' : 'neutral'}
              >
                {interaction.risk} risk
              </Badge>
            ) : null}
            {badge ? <Badge variant={badge.tone}>{badge.text}</Badge> : null}
            <span className="card-handle text-muted-foreground">{interaction.handle}</span>
          </CardAction>
        </CardHeader>
        {open ? (
          <CardContent className={cn(PART, 'flex flex-col gap-(--s3)')}>
            {props.children}
          </CardContent>
        ) : null}
        {open && props.footer ? (
          <CardFooter className={cn(PART, 'flex-col items-stretch gap-(--s3)')}>
            {props.footer}
          </CardFooter>
        ) : null}
        {status ? (
          <p className="card-status" data-tone={status.tone} role="status">
            {status.text}
          </p>
        ) : null}
        {props.error ? (
          <p className="card-error" role="alert">
            {props.error}
          </p>
        ) : null}
      </article>
    </Card>
  );
}
