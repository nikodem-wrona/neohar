#!/bin/sh
# Setup for git-log.steps: a git repository `repo` in the working directory, with 62 commits on
# five branches, merges, three tags, and fictional authors on example.invalid. The dates are
# fixed, so the repository does not depend on the day or on who makes it. Nothing here reads the
# user's git configuration. Each branch changes its own files, so that no merge has a conflict.
set -e

export GIT_CONFIG_NOSYSTEM=1
export GIT_CONFIG_GLOBAL=/dev/null
export GIT_TERMINAL_PROMPT=0

mkdir repo
cd repo
git init -q -b main
mkdir -p src docs

n=0

# The date of commit number $n: one commit per two hours, from 2026-01-05.
date_of() {
    day=$(( n / 12 + 5 ))
    month=$(( (day - 1) / 28 + 1 ))
    day=$(( (day - 1) % 28 + 1 ))
    printf '2026-%02d-%02dT%02d:17:00+0000' "$month" "$day" $(( (n % 12) * 2 ))
}

# add_lines FILE COUNT: appends COUNT lines to FILE.
add_lines() {
    i=1
    while [ "$i" -le "$2" ]; do
        printf 'line %s.%s of %s\n' "$n" "$i" "$1" >> "$1"
        i=$(( i + 1 ))
    done
}

# drop_lines FILE COUNT: removes the last COUNT lines of FILE.
drop_lines() {
    total=$(wc -l < "$1")
    keep=$(( total > $2 ? total - $2 : 0 ))
    head -n "$keep" "$1" > "$1.tmp"
    mv "$1.tmp" "$1"
}

# commit MESSAGE: commits what is staged, as author number $n mod 4.
commit() {
    case $(( n % 4 )) in
        0) name='Ada Example';  email='ada@example.invalid' ;;
        1) name='Ben Sample';   email='ben@example.invalid' ;;
        2) name='Chen Wei';     email='chen@example.invalid' ;;
        3) name='Dana Test';    email='dana@example.invalid' ;;
    esac
    GIT_AUTHOR_DATE=$(date_of) GIT_COMMITTER_DATE=$(date_of) \
        git -c user.name="$name" -c user.email="$email" commit -q -m "$1"
}

# step MESSAGE: a commit that changes two or three files of the current branch by a few lines.
step() {
    n=$(( n + 1 ))
    # symbolic-ref works before the first commit; rev-parse does not.
    case $(git symbolic-ref --short HEAD) in
        main)        code=src/lib.rs;     text=docs/notes.txt ;;
        feature-a)   code=src/feature_a.rs; text=docs/feature_a.txt ;;
        feature-b)   code=src/cache.rs;   text=docs/cache.txt ;;
        fix-unicode) code=src/unicode.rs; text=docs/unicode.txt ;;
        hotfix)      code=src/exit.rs;    text=docs/exit.txt ;;
    esac
    add_lines "$code" $(( n % 7 + 1 ))
    add_lines "$text" $(( n % 3 + 1 ))
    if [ $(( n % 4 )) -eq 0 ]; then
        add_lines "$code.extra" $(( n % 11 + 2 ))
    fi
    if [ $(( n % 5 )) -eq 0 ]; then
        drop_lines "$code" 2
    fi
    git add -A
    commit "$1"
}

# merge BRANCH: a merge commit.
merge() {
    n=$(( n + 1 ))
    GIT_AUTHOR_DATE=$(date_of) GIT_COMMITTER_DATE=$(date_of) \
        git -c user.name='Ada Example' -c user.email='ada@example.invalid' \
            merge -q --no-ff -m "Merge branch '$1'" "$1"
}

step 'Start the project'
step 'Add the parser'
step 'Add tests for the parser'
step 'Fix an off-by-one error in the lexer'
step 'Document the command line'
step 'Add a --verbose flag'
step 'Handle empty input'
step 'Speed up the lexer by 30%'
step 'Rename Token to Lexeme'
step 'Add 日本語 support to the lexer'
step 'Report errors with line and column'
step 'Refactor the error type'
step 'Add the formatter'
step 'Make the formatter configurable'
step 'Fix a crash on very long lines'
git tag v0.1

git checkout -q -b feature-a
step 'feature-a: sketch the API'
step 'feature-a: first working version'
step 'feature-a: add emoji output 🎉'
step 'feature-a: handle errors'
step 'feature-a: write the documentation'
step 'feature-a: address review comments'

git checkout -q main
step 'Fix the build on Linux'
step 'Update the notes'
step 'Bump the version to 0.2'
merge feature-a
step 'Clean up after the merge'
git tag v0.2

git checkout -q -b feature-b
step 'feature-b: start the cache'
step 'feature-b: add an eviction policy'
step 'feature-b: measure the cache'
step 'feature-b: fix a race in the cache'

git checkout -q main
git checkout -q -b fix-unicode
step 'fix-unicode: handle combining marks'
step 'fix-unicode: handle wide characters'
step 'fix-unicode: add tests'

git checkout -q main
step 'Remove dead code'
step 'Add a changelog'
merge fix-unicode
step 'Speed up the parser'
step 'Fix a typo in the manual'

git checkout -q feature-b
step 'feature-b: simplify the cache'
step 'feature-b: more tests'
step 'feature-b: final polish'

git checkout -q main
step 'Add a benchmark'
merge feature-b
step 'Fix the benchmark on slow machines'
step 'Update dependencies'
step 'Handle SIGPIPE'
step 'Add a man page'
step 'Prepare release 0.3'
git checkout -q -b hotfix
step 'hotfix: stop a crash on exit'
step 'hotfix: add a regression test'
git checkout -q main
merge hotfix
step 'Release 0.3'
# An annotated tag needs an identity. Without -c, git would look for one in the user database
# and the host name of this machine.
GIT_COMMITTER_DATE=$(date_of) git -c user.name='Ada Example' -c user.email='ada@example.invalid' \
    tag -a v0.3 -m 'Release 0.3'
step 'Start 0.4'
step 'Add the plugin hook'
step 'Document the plugin hook'
step 'Fix the plugin hook on Windows'
step 'Tidy the tests'
step 'Add a FAQ'
step 'Handle a corner case in the formatter'
step 'Add more tests for the formatter'
step 'Update the notes for 0.4'
step 'Fix a typo in the FAQ'
