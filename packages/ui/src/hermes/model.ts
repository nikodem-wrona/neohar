// What the Hermes view shows for the facts of `hermes_chats`: the words of a state, its tone, and
// what the composer can do. The core gives the facts; this file only names them.

import type { HermesAnswerState, HermesChatValue, HermesToolStatus } from '@neohar/transport';

export type Tone = 'live' | 'caution' | 'danger' | 'dim';

/** The word of the state of a chat, and its tone. */
export function stateOf(chat: Pick<HermesChatValue, 'state' | 'guessed' | 'pending'>): {
  label: string;
  tone: Tone;
} {
  switch (chat.state) {
    case 'starting':
      return { label: 'Starting', tone: 'dim' };
    case 'idle':
      return { label: 'Idle', tone: 'live' };
    case 'working':
      return { label: chat.guessed ? 'Working?' : 'Working', tone: 'caution' };
    case 'waiting':
      return {
        label: (chat.pending ?? 0) > 1 ? `${chat.pending} approvals` : 'Approval',
        tone: 'danger',
      };
    case 'disconnected':
      return { label: 'Disconnected', tone: 'danger' };
  }
}

/** What the composer can do now. A prompt during a turn goes to the queue of Hermes. */
export function composer(chat: Pick<HermesChatValue, 'state'>): {
  send: boolean;
  stop: boolean;
  note: string | null;
} {
  switch (chat.state) {
    case 'starting':
      return { send: true, stop: false, note: 'Hermes starts. A prompt waits for the session.' };
    case 'working':
      return { send: true, stop: true, note: 'Hermes works. A new prompt waits for this turn.' };
    case 'waiting':
      return { send: true, stop: true, note: 'An approval waits for you.' };
    case 'disconnected':
      return { send: true, stop: false, note: null };
    case 'idle':
      return { send: true, stop: false, note: null };
  }
}

/** The mark of the status of a tool call. */
export function toolMark(status: HermesToolStatus): { mark: string; tone: Tone } {
  switch (status) {
    case 'running':
      return { mark: '…', tone: 'caution' };
    case 'done':
      return { mark: '✓', tone: 'live' };
    case 'failed':
      return { mark: '✕', tone: 'danger' };
  }
}

/** The line under an approval that is no longer open. */
export function answerLine(state: HermesAnswerState, answer: string | undefined): string | null {
  switch (state) {
    case 'open':
      return null;
    case 'answered':
      return `Answered: ${answer ?? ''}`.trim();
    case 'expired':
      return 'Nobody answered in time, so Hermes denied it.';
    case 'gone':
      return 'The request ended without an answer.';
  }
}

/** The name of the last folder of a path, for a button. */
export function folderName(path: string): string {
  const parts = path.split('/').filter((part) => part !== '');
  return parts.at(-1) ?? path;
}
