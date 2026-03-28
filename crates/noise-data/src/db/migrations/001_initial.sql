-- ============================================================
-- Migration 001: Initial schema
-- ============================================================

CREATE TABLE IF NOT EXISTS projects (
    id          TEXT PRIMARY KEY,   -- UUID
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    crs_epsg    INTEGER NOT NULL DEFAULT 4326,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS scenarios (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    is_base     INTEGER NOT NULL DEFAULT 0,  -- 1 = base scenario
    parent_id   TEXT REFERENCES scenarios(id)
);

CREATE TABLE IF NOT EXISTS scene_objects (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    scenario_id TEXT NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
    object_type TEXT NOT NULL,    -- 'road_source', 'building', 'barrier', 'receiver', etc.
    name        TEXT NOT NULL DEFAULT '',
    geometry_wkt TEXT,            -- WKT geometry for quick spatial queries
    data_json   TEXT NOT NULL     -- Full serialized object as JSON
);

CREATE INDEX IF NOT EXISTS idx_scene_objects_scenario ON scene_objects(scenario_id);
CREATE INDEX IF NOT EXISTS idx_scene_objects_type     ON scene_objects(object_type);

CREATE TABLE IF NOT EXISTS calculation_results (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    scenario_id     TEXT NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
    grid_type       TEXT NOT NULL,  -- 'horizontal', 'vertical', 'facade'
    metric          TEXT NOT NULL,  -- 'Ld', 'Ln', 'Lden', etc.
    calculated_at   TEXT NOT NULL,
    result_json     TEXT NOT NULL   -- Grid results as JSON
);

CREATE TABLE IF NOT EXISTS users (
    id              TEXT PRIMARY KEY,  -- UUID
    username        TEXT NOT NULL UNIQUE,
    password_hash   TEXT NOT NULL,
    email           TEXT NOT NULL UNIQUE,
    role            TEXT NOT NULL DEFAULT 'viewer',  -- 'admin', 'analyst', 'viewer'
    created_at      TEXT NOT NULL,
    last_login_at   TEXT
);

CREATE TABLE IF NOT EXISTS sessions (
    token           TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at      TEXT NOT NULL,
    created_at      TEXT NOT NULL
);
