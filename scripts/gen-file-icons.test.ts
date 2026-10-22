import { expect, test } from 'bun:test';
import { readIcon, table } from './gen-file-icons';

const svg = (body: string, attrs = ''): string =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"${attrs}>${body}</svg>`;

test('an icon keeps the fill, the data and the fill rule of each path', () => {
  expect(
    readIcon(
      svg(
        '<path fill="#0288d1" d="M2 2v12h12V2z"/><path fill="#fff" fill-rule="evenodd" d="M4 4h1"/>',
      ),
    ),
  ).toEqual({
    viewBox: '0 0 16 16',
    paths: [
      ['#0288d1', 'M2 2v12h12V2z'],
      ['#fff', 'M4 4h1', 1],
    ],
  });
});

test('a group passes its fill on, and a shape with no fill only sizes the icon', () => {
  expect(
    readIcon(
      svg('<path d="M0 0h24v24H0z"/><g fill="#42a5f5"><path d="M1 1h2"/></g>', ' fill="none"'),
    ),
  ).toEqual({ viewBox: '0 0 16 16', paths: [['#42a5f5', 'M1 1h2']] });
});

test('a circle, an ellipse and a rectangle become paths', () => {
  const icon = readIcon(
    svg(
      '<circle cx="8" cy="8" r="2" fill="#f00"/><ellipse cx="4" cy="4" rx="2" ry="1" fill="#0f0"/>' +
        '<rect x="1" y="2" width="4" height="3" fill="#00f"/>',
    ),
  );
  expect(icon).toEqual({
    viewBox: '0 0 16 16',
    paths: [
      ['#f00', 'M6 8a2 2 0 1 0 4 0a2 2 0 1 0 -4 0z'],
      ['#0f0', 'M2 4a2 1 0 1 0 4 0a2 1 0 1 0 -4 0z'],
      ['#00f', 'M1 2h4v3h-4z'],
    ],
  });
});

test('a gradient, a stroke or a transform is refused with its reason', () => {
  expect(readIcon(svg('<defs><linearGradient id="a"/></defs>'))).toBe('it has <defs>');
  expect(readIcon(svg('<path fill="#000" stroke="#fff" d="M0 0"/>'))).toBe('<path> has stroke');
  expect(readIcon(svg('<g transform="scale(2)"><path fill="#000" d="M0 0"/></g>'))).toBe(
    '<g> has transform',
  );
  expect(readIcon(svg('<path d="M0 0"/>'))).toBe('a <path> has the fill undefined');
});

test('a table keeps the names of the chosen icons, in lower case, by their bytes', () => {
  expect(
    table(
      {
        Dockerfile: 'docker',
        dockerfile: 'docker',
        'b.json': 'json',
        'a.txt': 'document',
        x: 'gone',
      },
      new Set(['docker', 'json', 'document']),
    ),
  ).toEqual([
    ['a.txt', 'document'],
    ['b.json', 'json'],
    ['dockerfile', 'docker'],
  ]);
});
