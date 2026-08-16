PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    name TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT PRIMARY KEY,
    task_id TEXT REFERENCES tasks(id),
    path TEXT NOT NULL,
    sha256 TEXT,
    created_at TEXT NOT NULL,
    metadata_json TEXT
);

CREATE TABLE IF NOT EXISTS notes (
    id TEXT PRIMARY KEY,
    task_id TEXT REFERENCES tasks(id),
    body TEXT NOT NULL,
    created_at TEXT NOT NULL
);
