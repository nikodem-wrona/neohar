<!--
  The title is the header of the squashed commit on main, so it is a Conventional Commit:
  "<type>(<scope>): <description>", for example "fix(core): keep the holder of a moved pane".
  Give it a "!" for a breaking change. See "Commit messages" in CONTRIBUTING.md.
-->

## What this pull request changes

<!-- Write this text in Simplified Technical English. Tell what changes and why. -->

## Evidence

<!-- The tests that you added, and the results of the local suites that cover the change. -->

## Checklist

- [ ] `mise run check` passes.
- [ ] `mise run test` passes.
- [ ] I ran the local suites that cover the change, and I put their results above.
- [ ] I updated the guide pages in `website/docs/` that the change makes wrong. For an API change, I ran `mise run gen:api` and `bun run gen` in `website/`, and I accepted the snapshot.
- [ ] The title and the commit messages are Conventional Commits in Simplified Technical English.
- [ ] A breaking change has a `!` in the title and a `BREAKING CHANGE:` footer in its commit.
