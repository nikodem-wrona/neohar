// The History side of the Git view: the commits of `HEAD`, the newest first, in pages of 200
// (`git.log`), with a mark on each commit that a push would send. A pick shows the commit with its
// message and its files (`git.show`), and a pick of a file shows its diff from the first parent
// (`git.diff` with the commit). A new `HEAD` reads the history again from the start.

import { type GitCommit, type GitLogResult, type GitShown, RpcError } from '@neohar/transport';
import { type KeyboardEvent, useCallback, useEffect, useRef, useState } from 'react';
import { useApp } from '../app/context';
import { Button } from '../components/ui/button';
import { Input } from '../components/ui/input';
import { ScrollArea } from '../components/ui/scroll-area';
import { SCROLL_LIST as LIST } from './scroll';
import { statusLetter, statusWord } from '../changes/model';
import { cn } from '../lib/utils';
import { DiffPane, type Loaded } from './DiffPane';
import { commitLabels, matches, moveIndex } from './model';

function errorText(failure: unknown, what: string): string {
  return failure instanceof RpcError ? failure.message : `${what} did not load.`;
}

export function GitHistory(props: {
  /** The commit of `HEAD`. Another one reads the history again. */
  head: string | null;
  /** The worktree. Another one reads the history again. */
  subject: string | null;
}) {
  const { transport, report, windowId } = useApp();
  const [commits, setCommits] = useState<GitCommit[]>([]);
  const [more, setMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const [filter, setFilter] = useState('');
  const [picked, setPicked] = useState<string | null>(null);
  const [shown, setShown] = useState<GitShown | { error: string } | null>(null);
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const list = useRef<HTMLUListElement>(null);
  /** Grows with each new start, so that a page of an older start is dropped. */
  const generation = useRef(0);
  /** The picked commit now, for an answer that comes after the pick. */
  const pickedNow = useRef<string | null>(null);
  const choose = (hash: string | null): void => {
    pickedNow.current = hash;
    setPicked(hash);
  };

  const page = useCallback(
    (skip: number): void => {
      const at = generation.current;
      setLoading(true);
      transport
        .call('git.log', { window: windowId, skip })
        .then((result: GitLogResult) => {
          if (at !== generation.current) return;
          setCommits((before) => (skip === 0 ? result.commits : [...before, ...result.commits]));
          setMore(result.more);
          setFailed(null);
        })
        .catch((failure: unknown) => {
          if (at === generation.current) setFailed(errorText(failure, 'The history'));
        })
        .finally(() => {
          if (at === generation.current) setLoading(false);
        })
        .catch(report);
    },
    [transport, windowId, report],
  );

  const { subject, head } = props;
  useEffect(() => {
    generation.current += 1;
    setCommits([]);
    setMore(false);
    pickedNow.current = null;
    setPicked(null);
    setShown(null);
    setLoaded(null);
    if (subject) page(0);
  }, [subject, head, page]);

  const pick = (hash: string): void => {
    choose(hash);
    setShown(null);
    setLoaded(null);
    transport
      .call('git.show', { window: windowId, commit: hash })
      .then((result) => {
        if (pickedNow.current === hash) setShown(result);
      })
      .catch((failure: unknown) => {
        if (pickedNow.current === hash) setShown({ error: errorText(failure, 'The commit') });
      })
      .catch(report);
  };

  const pickFile = (commit: string, file: string): void => {
    setLoaded({ file });
    transport
      .call('git.diff', { window: windowId, file, commit })
      .then((diff) => ({ diff }))
      .catch((failure: unknown) => ({ error: errorText(failure, 'The diff') }))
      .then((result) => {
        setLoaded((before) =>
          before?.file === file && pickedNow.current === commit ? { file, ...result } : before,
        );
      })
      .catch(report);
  };

  const visible = commits.filter((commit) =>
    matches(filter, [commit.subject, commit.author, commit.short, ...commitLabels(commit)]),
  );

  const onListKey = (event: KeyboardEvent): void => {
    const rows = visible;
    const at = rows.findIndex((commit) => commit.hash === picked);
    const to = moveIndex(rows.length, at, event.key);
    if (to === null) return;
    event.preventDefault();
    const hash = rows[to]?.hash;
    if (hash === undefined) return;
    pick(hash);
    list.current?.querySelector<HTMLElement>(`[data-hash="${hash}"]`)?.focus();
  };

  const detail = shown && 'commit' in shown ? shown : null;

  return (
    <>
      <div className="git-side-body">
        <Input
          type="text"
          placeholder="Filter"
          aria-label="Filter the commits"
          className="h-auto flex-none rounded-sm px-2 py-0.5 text-sm/5 shadow-none md:text-sm/5"
          value={filter}
          onChange={(event) => setFilter(event.currentTarget.value)}
        />
        {failed ? (
          <p className="git-error" role="alert">
            {failed}
          </p>
        ) : null}
        {visible.length > 0 ? (
          <ScrollArea className={LIST}>
            <ul className="git-list" aria-label="Commits" ref={list} onKeyDown={onListKey}>
              {visible.map((commit) => (
                <li key={commit.hash}>
                  <button
                    type="button"
                    className={cn('git-commit', { picked: picked === commit.hash })}
                    data-hash={commit.hash}
                    aria-pressed={picked === commit.hash}
                    tabIndex={
                      picked === commit.hash || (picked === null && commit === visible[0]) ? 0 : -1
                    }
                    onClick={() => pick(commit.hash)}
                  >
                    <span className="git-commit-subject">
                      {commit.unpushed ? (
                        <span className="git-unpushed" title="Not pushed">
                          ↑
                        </span>
                      ) : null}
                      {commit.subject}
                    </span>
                    <span className="git-commit-meta">
                      {commit.author} · {commit.time_text} ·{' '}
                      <span className="git-hash">{commit.short}</span>
                    </span>
                    {commitLabels(commit).length > 0 ? (
                      <span className="git-labels">
                        {commitLabels(commit).map((label) => (
                          <span key={label} className="git-label">
                            {label}
                          </span>
                        ))}
                      </span>
                    ) : null}
                  </button>
                </li>
              ))}
            </ul>
            {more ? (
              <Button
                variant="ghost"
                size="xs"
                className="self-start text-sm font-normal text-muted-foreground"
                disabled={loading}
                onClick={() => page(commits.length)}
              >
                {loading ? 'Loading…' : 'Show more commits'}
              </Button>
            ) : null}
          </ScrollArea>
        ) : (
          <p className="git-empty">
            {loading ? 'Loading…' : filter ? 'No commit matches.' : 'No commit yet.'}
          </p>
        )}
      </div>
      <div className="git-detail">
        {picked ? (
          detail ? (
            <div className="git-commit-detail">
              <header className="git-commit-head">
                <h2 className="git-commit-title">{detail.commit.subject}</h2>
                <p className="git-commit-meta">
                  {detail.commit.author} · {detail.commit.time_text} ·{' '}
                  <span className="git-hash">{detail.commit.short}</span>
                  {detail.parents.length > 1 ? <> · merge of {detail.parents.join(', ')}</> : null}
                </p>
                {detail.body ? <pre className="git-commit-body">{detail.body}</pre> : null}
              </header>
              <ScrollArea
                className={cn(LIST, 'flex-none [&>[data-slot=scroll-area-viewport]]:max-h-[25vh]')}
              >
                <ul className="git-list" aria-label="Files of the commit">
                  {detail.files.map((row) => (
                    <li key={row.path}>
                      <button
                        type="button"
                        className={cn('git-file', { picked: loaded?.file === row.path })}
                        aria-pressed={loaded?.file === row.path}
                        title={row.old_path ? `${row.old_path} → ${row.path}` : row.path}
                        onClick={() => pickFile(picked, row.path)}
                      >
                        <span
                          className="changes-status"
                          data-status={row.status}
                          title={statusWord(row.status)}
                        >
                          <span aria-hidden="true">{statusLetter(row.status)}</span>
                          <span className="visually-hidden">{statusWord(row.status)}</span>
                        </span>
                        <span className="changes-path">
                          <span className="changes-dir">{row.dir}</span>
                          <span className="changes-name">{row.name}</span>
                        </span>
                        <span className="git-lines">
                          {!row.binary ? (
                            <>
                              <span className="add">+{row.add}</span>{' '}
                              <span className="del">−{row.del}</span>
                            </>
                          ) : (
                            <span className="changes-binary">binary</span>
                          )}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              </ScrollArea>
              {detail.truncated ? (
                <p className="git-note">The list shows the first 1,000 files.</p>
              ) : null}
              <DiffPane loaded={loaded} empty="Pick a file to see its diff." />
            </div>
          ) : (
            <p className={shown ? 'git-error' : 'git-empty'} role="status">
              {shown && 'error' in shown ? shown.error : 'Loading…'}
            </p>
          )
        ) : (
          <p className="git-empty">Pick a commit to see its message and its files.</p>
        )}
      </div>
    </>
  );
}
