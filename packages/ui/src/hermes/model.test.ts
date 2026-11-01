import { describe, expect, test } from 'bun:test';
import { answerLine, composer, folderName, stateOf, toolMark } from './model';

describe('the Hermes view model', () => {
  test('a state has a word and a tone', () => {
    expect(stateOf({ state: 'idle', guessed: false, pending: 0 })).toEqual({
      label: 'Idle',
      tone: 'live',
    });
    expect(stateOf({ state: 'working', guessed: true, pending: 0 }).label).toBe('Working?');
    expect(stateOf({ state: 'waiting', guessed: false, pending: 2 })).toEqual({
      label: '2 approvals',
      tone: 'danger',
    });
    expect(stateOf({ state: 'disconnected', guessed: false, pending: 0 }).tone).toBe('danger');
  });

  test('the composer can stop only a turn', () => {
    expect(composer({ state: 'idle' })).toEqual({ send: true, stop: false, note: null });
    expect(composer({ state: 'working' }).stop).toBe(true);
    expect(composer({ state: 'waiting' }).note).toBe('An approval waits for you.');
  });

  test('a tool and an answer have their marks', () => {
    expect(toolMark('done').mark).toBe('✓');
    expect(toolMark('failed').tone).toBe('danger');
    expect(answerLine('open', undefined)).toBeNull();
    expect(answerLine('answered', 'Allow once')).toBe('Answered: Allow once');
    expect(answerLine('expired', undefined)).toContain('denied');
  });

  test('a folder has a short name', () => {
    expect(folderName('/Users/me/project/')).toBe('project');
    expect(folderName('/')).toBe('/');
  });
});
