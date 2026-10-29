import type { TimelineBody, TimelineItem } from '@neohar/transport';
import { describe, expect, test } from 'bun:test';
import {
  groupMessages,
  missingTexts,
  moveChoice,
  skillQuery,
  TEXTS_PER_CALL,
  textKey,
  workingText,
} from './model';

function item(id: string, body: TimelineBody, turn = 1): TimelineItem {
  return { id, at: '2026-10-28T09:00:00Z', turn, body };
}

const prompt = (id: string, preview = 'do it') => item(id, { type: 'prompt', preview });
const reply = (id: string, preview = 'done') => item(id, { type: 'assistant', preview });
const tool = (id: string, status: 'running' | 'ok') =>
  item(id, { type: 'tool', tool: 'Bash', status, summary: 'cargo test' });

describe('groupMessages', () => {
  test('a prompt stands alone and the items of the agent after it go together', () => {
    const items = [reply('a0.0'), prompt('u1'), tool('t1', 'ok'), reply('a1.0'), prompt('u2')];
    expect(groupMessages(items).map((group) => [group.kind, group.items.map((i) => i.id)])).toEqual(
      [
        ['agent', ['a0.0']],
        ['prompt', ['u1']],
        ['agent', ['t1', 'a1.0']],
        ['prompt', ['u2']],
      ],
    );
  });
});

describe('textKey', () => {
  test('a prompt and a message have a key that changes with the preview', () => {
    expect(textKey(prompt('u1'))).toBe('u1\ndo it');
    expect(textKey(reply('a1.0', 'more'))).toBe('a1.0\nmore');
    expect(textKey(tool('t1', 'ok'))).toBeNull();
  });
});

describe('missingTexts', () => {
  test('gives the texts that are not known, the latest first', () => {
    const items = [prompt('u1'), tool('t1', 'ok'), reply('a1.0'), prompt('u2')];
    const known = new Set([textKey(items[0]!)]);
    expect(missingTexts(items, (key) => known.has(key)).map((i) => i.id)).toEqual(['u2', 'a1.0']);
  });

  test('asks for at most one call', () => {
    const items = Array.from({ length: TEXTS_PER_CALL + 5 }, (_, n) => prompt(`u${n}`));
    const missing = missingTexts(items, () => false);
    expect(missing).toHaveLength(TEXTS_PER_CALL);
    expect(missing[0]?.id).toBe(`u${TEXTS_PER_CALL + 4}`);
  });
});

describe('workingText', () => {
  test('names the tool of this turn that runs', () => {
    expect(workingText([prompt('u1'), tool('t1', 'running')])).toBe('Working: Bash cargo test');
  });

  test('a tool of an earlier turn or a finished tool does not count', () => {
    expect(workingText([tool('t1', 'running'), prompt('u2')])).toBe('Working');
    expect(workingText([prompt('u1'), tool('t1', 'ok')])).toBe('Working');
  });
});

describe('skillQuery', () => {
  test('reads the name after / or $ until the first space', () => {
    expect(skillQuery('/')).toBe('');
    expect(skillQuery('/rev')).toBe('rev');
    expect(skillQuery('$commit')).toBe('commit');
    expect(skillQuery('/review now')).toBeNull();
    expect(skillQuery('fix /this')).toBeNull();
    expect(skillQuery('')).toBeNull();
  });
});

describe('moveChoice', () => {
  test('goes round at the ends', () => {
    expect(moveChoice(0, -1, 3)).toBe(2);
    expect(moveChoice(2, 1, 3)).toBe(0);
    expect(moveChoice(0, 1, 0)).toBe(0);
  });
});
