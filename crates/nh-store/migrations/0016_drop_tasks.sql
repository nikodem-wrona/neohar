-- neohar has no task workspaces any more. This migration removes their tables, the task of each
-- run, the favorite folders of the form of a new task, and the count of references of the blobs
-- (only the files of tasks counted them). The worktrees and the branches of the tasks stay on
-- the disk, and the blobs stay in the blob store.

DROP TABLE port_leases;
DROP TABLE tasks;
DROP TABLE favorite_folders;

ALTER TABLE agent_runs DROP COLUMN task_id;
ALTER TABLE blobs DROP COLUMN refs;

DELETE FROM handle_counters WHERE scope = 'task';
DELETE FROM projection_versions WHERE name IN ('tasks', 'favorites') OR name LIKE 'changes:%';
