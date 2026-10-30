-- neohar has no search of the scrollback any more. This migration removes the full-text index of
-- the archived lines. The segment files of the archive and their table stay: the scrollback of a
-- pane still reads them.

DROP TABLE scrollback_fts;
