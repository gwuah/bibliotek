//! The v1 schema is Bibliotek's own migrations copied verbatim; they are frozen.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use libsql::params::IntoParams;
use libsql::{Builder, Connection};
use serde_json::json;

pub async fn create_database(path: &Path) -> Connection {
    Builder::new_local(path)
        .build()
        .await
        .expect("create fixture database")
        .connect()
        .expect("connect to fixture database")
}

pub async fn execute(conn: &Connection, sql: &str, params: impl IntoParams) {
    conn.execute(sql, params)
        .await
        .unwrap_or_else(|error| panic!("{sql}: {error}"));
}

pub async fn count(conn: &Connection, sql: &str) -> i64 {
    let mut rows = conn
        .query(sql, ())
        .await
        .unwrap_or_else(|error| panic!("{sql}: {error}"));
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

pub const RESEARCH_SCHEMA: &str = "
    CREATE TABLE items (id TEXT PRIMARY KEY, title TEXT, deleted_at TEXT);
    CREATE TABLE annotations (
        id TEXT PRIMARY KEY, item_id TEXT NOT NULL, content TEXT, color TEXT, position TEXT
    );
    CREATE TABLE comments (id TEXT PRIMARY KEY, annotation_id TEXT NOT NULL, content TEXT);
    CREATE TABLE notes (id TEXT PRIMARY KEY, item_id TEXT NOT NULL, content TEXT);
";

pub fn research_position(page: i64) -> String {
    json!({
        "boundingRect": { "x1": 10.5, "y1": 20.0, "x2": 110.0, "y2": 32.0, "width": 600, "height": 800, "pageNumber": page },
        "rects": [{ "x1": 10.5, "y1": 20.0, "x2": 110.0, "y2": 32.0, "width": 600, "height": 800, "pageNumber": page }]
    })
    .to_string()
}

pub struct ResearchDb {
    pub path: PathBuf,
    pub conn: Connection,
}

impl ResearchDb {
    pub async fn create(path: PathBuf) -> ResearchDb {
        let conn = create_database(&path).await;
        conn.execute_batch(RESEARCH_SCHEMA)
            .await
            .expect("create Research schema");
        ResearchDb { path, conn }
    }

    pub async fn item(&self, id: &str, title: &str) {
        execute(
            &self.conn,
            "INSERT INTO items (id, title) VALUES (?, ?)",
            [id, title],
        )
        .await;
    }

    pub async fn annotation(&self, id: &str, item_id: &str, text: &str, page: i64) {
        let content = json!({ "text": text }).to_string();
        execute(
            &self.conn,
            "INSERT INTO annotations (id, item_id, content, color, position) VALUES (?, ?, ?, '#ffd400', ?)",
            [id, item_id, content.as_str(), research_position(page).as_str()],
        )
        .await;
    }

    pub async fn comment(&self, id: &str, annotation_id: &str, content: &str) {
        execute(
            &self.conn,
            "INSERT INTO comments (id, annotation_id, content) VALUES (?, ?, ?)",
            [id, annotation_id, content],
        )
        .await;
    }

    pub async fn note(&self, id: &str, item_id: &str, content: &str) {
        execute(
            &self.conn,
            "INSERT INTO notes (id, item_id, content) VALUES (?, ?, ?)",
            [id, item_id, content],
        )
        .await;
    }

    pub async fn run(&self, sql: &str) {
        execute(&self.conn, sql, ()).await;
    }
}

pub const V1_MIGRATIONS: &[(&str, &str)] = &[
    ("system/000_migrations_table.sql", V1_SYSTEM),
    ("001_schema.sql", V1_LIBRARY),
    (
        "003_add_book_status.sql",
        "ALTER TABLE books ADD COLUMN status TEXT DEFAULT 'complete';",
    ),
    ("commonplace_001_schema.sql", V1_COMMONPLACE_001),
    ("commonplace_002_external_id.sql", V1_COMMONPLACE_002),
    ("commonplace_003_sync_metadata.sql", V1_COMMONPLACE_003),
    (
        "commonplace_004_resource_config.sql",
        "ALTER TABLE resources ADD COLUMN config TEXT;",
    ),
    ("research_001_config.sql", V1_RESEARCH_001),
    // From Bibliotek's deleted `reading` branch; only its presence is known, not its shape.
    ("reader_001_settings.sql", V1_STALE_READER),
];

const V1_SYSTEM: &str = "
CREATE TABLE IF NOT EXISTS _migrations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_migrations_name ON _migrations (name);
";

const V1_LIBRARY: &str = "
CREATE TABLE IF NOT EXISTS books (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL UNIQUE,
    url TEXT NOT NULL UNIQUE,
    cover_url TEXT UNIQUE,
    ratings INTEGER,
    description TEXT,
    pages INTEGER,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE TABLE IF NOT EXISTS categories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    parent_id INTEGER REFERENCES categories(id),
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    parent_id INTEGER REFERENCES tags(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE TABLE IF NOT EXISTS authors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE TABLE IF NOT EXISTS book_authors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    book_id INTEGER,
    author_id INTEGER,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (book_id) REFERENCES books (id),
    FOREIGN KEY (author_id) REFERENCES authors (id)
);
CREATE TABLE IF NOT EXISTS book_categories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    book_id INTEGER,
    category_id INTEGER,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (book_id) REFERENCES books (id),
    FOREIGN KEY (category_id) REFERENCES categories (id)
);
CREATE TABLE IF NOT EXISTS book_tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    book_id INTEGER,
    tag_id INTEGER,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (book_id) REFERENCES books (id),
    FOREIGN KEY (tag_id) REFERENCES tags (id)
);
DELETE FROM book_authors WHERE id NOT IN (
    SELECT MIN(id) FROM book_authors GROUP BY book_id, author_id
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_book_author_unique ON book_authors (book_id, author_id);
DELETE FROM book_categories WHERE id NOT IN (
    SELECT MIN(id) FROM book_categories GROUP BY book_id, category_id
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_book_category_unique ON book_categories (book_id, category_id);
DELETE FROM book_tags WHERE id NOT IN (
    SELECT MIN(id) FROM book_tags GROUP BY book_id, tag_id
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_book_tag_unique ON book_tags (book_id, tag_id);
";

const V1_COMMONPLACE_001: &str = "
CREATE TABLE IF NOT EXISTS resources (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    type TEXT NOT NULL CHECK (type IN ('website', 'pdf')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE TABLE IF NOT EXISTS annotations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    resource_id INTEGER NOT NULL,
    text TEXT NOT NULL,
    color TEXT,
    boundary TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (resource_id) REFERENCES resources (id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS comments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    annotation_id INTEGER NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (annotation_id) REFERENCES annotations (id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS notes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    resource_id INTEGER NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (resource_id) REFERENCES resources (id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS words (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    resource_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    meaning TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (resource_id) REFERENCES resources (id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_annotations_resource_id ON annotations (resource_id);
CREATE INDEX IF NOT EXISTS idx_comments_annotation_id ON comments (annotation_id);
CREATE INDEX IF NOT EXISTS idx_notes_resource_id ON notes (resource_id);
CREATE INDEX IF NOT EXISTS idx_words_resource_id ON words (resource_id);
CREATE INDEX IF NOT EXISTS idx_words_name ON words (name);
";

const V1_COMMONPLACE_002: &str = "
ALTER TABLE resources ADD COLUMN external_id TEXT;
ALTER TABLE annotations ADD COLUMN external_id TEXT;
ALTER TABLE notes ADD COLUMN external_id TEXT;
ALTER TABLE comments ADD COLUMN external_id TEXT;
CREATE INDEX IF NOT EXISTS idx_resources_external_id ON resources (external_id);
CREATE INDEX IF NOT EXISTS idx_annotations_external_id ON annotations (external_id);
CREATE INDEX IF NOT EXISTS idx_notes_external_id ON notes (external_id);
CREATE INDEX IF NOT EXISTS idx_comments_external_id ON comments (external_id);
";

const V1_COMMONPLACE_003: &str = "
ALTER TABLE resources ADD COLUMN content_hash TEXT;
ALTER TABLE annotations ADD COLUMN content_hash TEXT;
ALTER TABLE notes ADD COLUMN content_hash TEXT;
ALTER TABLE comments ADD COLUMN content_hash TEXT;
ALTER TABLE resources ADD COLUMN deleted_at TEXT;
ALTER TABLE annotations ADD COLUMN deleted_at TEXT;
ALTER TABLE notes ADD COLUMN deleted_at TEXT;
ALTER TABLE comments ADD COLUMN deleted_at TEXT;
CREATE INDEX IF NOT EXISTS idx_resources_content_hash ON resources (content_hash);
CREATE INDEX IF NOT EXISTS idx_annotations_content_hash ON annotations (content_hash);
CREATE INDEX IF NOT EXISTS idx_notes_content_hash ON notes (content_hash);
CREATE INDEX IF NOT EXISTS idx_comments_content_hash ON comments (content_hash);
CREATE INDEX IF NOT EXISTS idx_resources_deleted_at ON resources (deleted_at);
CREATE INDEX IF NOT EXISTS idx_annotations_deleted_at ON annotations (deleted_at);
CREATE INDEX IF NOT EXISTS idx_notes_deleted_at ON notes (deleted_at);
CREATE INDEX IF NOT EXISTS idx_comments_deleted_at ON comments (deleted_at);
";

const V1_RESEARCH_001: &str = "
CREATE TABLE IF NOT EXISTS research_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    db_path TEXT NOT NULL,
    last_sync_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
";

const V1_STALE_READER: &str = "
CREATE TABLE reader_settings (id INTEGER PRIMARY KEY, target_language TEXT, api_key TEXT);
CREATE TABLE page_texts (id INTEGER PRIMARY KEY, resource_id INTEGER, page INTEGER, text TEXT);
";

pub struct V1Db {
    pub path: PathBuf,
    pub conn: Connection,
}

impl V1Db {
    pub async fn create(path: PathBuf) -> V1Db {
        let conn = create_database(&path).await;
        for (name, sql) in V1_MIGRATIONS {
            conn.execute_batch(sql)
                .await
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            execute(&conn, "INSERT INTO _migrations (name) VALUES (?)", [*name]).await;
        }
        V1Db { path, conn }
    }

    pub async fn run(&self, sql: &str, params: impl IntoParams) {
        execute(&self.conn, sql, params).await;
    }
}

pub mod v1 {
    pub const ESSAY_URL: &str = "https://example.com/essay";
    pub const OTHER_URL: &str = "https://example.com/other";
    pub const EMPTY_URL: &str = "https://example.com/empty";
    pub const ESSAY_CHUNKS: &[&str] = &["The struggle ", "of memory"];
    pub const ESSAY_DATE: &str = "2025-03-01T10:00:00.000Z";
    pub const ESSAY_TEXT: &str = "The struggle of memory";
    pub const ESSAY_COMMENT: &str = "against forgetting";
    pub const RESEARCH_TITLE: &str = "Decolonising the Mind";
}

pub async fn standard_v1(db: &V1Db) {
    let run = |sql: &'static str| db.run(sql, ());
    run("INSERT INTO books (id, title, url, description, created_at, updated_at) VALUES
         (1, 'Our Sister Killjoy', 'https://bibliotek.t3.storage.dev/0123456789abcdef_Our%20Sister%20Killjoy.pdf',
          'A novel in four movements', '2024-01-01T00:00:00.000Z', '2024-01-02T00:00:00.000Z'),
         (2, 'The Trial of Dedan Kimathi', 'https://bibliotek.t3.storage.dev/1111111111111111_The%20Trial%20%26%20After.pdf',
          NULL, '2024-02-01T00:00:00.000Z', '2024-02-01T00:00:00.000Z'),
         (3, 'Decolonising the Mind', 'https://minio.storage.dev/2222222222222222_Decolonising%20the%20Mind.pdf',
          '', '2024-03-01T00:00:00.000Z', '2024-03-01T00:00:00.000Z')")
    .await;
    run("INSERT INTO authors (id, name) VALUES
         (1, 'Ama Ata Aidoo'), (2, 'Ngũgĩ wa Thiong''o'), (3, 'Micere Githae Mugo'), (4, 'Unused Author')")
    .await;
    run("INSERT INTO book_authors (book_id, author_id) VALUES (1, 1), (2, 2), (2, 3), (3, 2)").await;
    run("INSERT INTO categories (id, name, parent_id) VALUES
         (1, 'humanities', NULL), (2, 'literature', 1), (3, 'history', 1), (4, 'african literature', 2),
         (5, 'physics', NULL)")
    .await;
    run("INSERT INTO book_categories (book_id, category_id) VALUES (1, 4), (2, 2), (2, 3), (3, 3)").await;
    run("INSERT INTO tags (id, name) VALUES (1, 'fiction'), (2, 'drama'), (3, 'undefined'), (4, 'HP LaserJet 600 dpi')")
        .await;
    run("INSERT INTO book_tags (book_id, tag_id) VALUES (1, 1), (2, 2), (2, 3), (3, 4)").await;

    run("INSERT INTO resources (id, title, type, external_id, config, deleted_at, created_at) VALUES
         (1, 'https://example.com/essay', 'website', NULL, NULL, NULL, '2025-03-01T10:00:01.000Z'),
         (2, 'https://example.com/other', 'website', NULL, NULL, NULL, '2023-05-01T00:00:00.000Z'),
         (3, 'Decolonising the Mind', 'pdf', 'research:item-1',
          '{\"chapters\": {\"1\": [\"Introduction\", 1], \"2\": [\"The Language of African Literature\", 10]}}',
          NULL, '2025-01-01T00:00:00.000Z'),
         (4, 'Some Paper', 'pdf', 'research:item-2', NULL, NULL, '2025-01-02T00:00:00.000Z'),
         (5, 'https://example.com/empty', 'website', NULL, NULL, NULL, '2025-01-03T00:00:00.000Z'),
         (6, 'https://example.com/gone', 'website', NULL, NULL, '2025-02-01T00:00:00.000Z', '2025-01-04T00:00:00.000Z')")
    .await;

    let essay_boundary = serde_json::json!({
        "groupID": 1700000000001_i64, "date": v1::ESSAY_DATE, "chunks": v1::ESSAY_CHUNKS, "url": v1::ESSAY_URL,
    })
    .to_string();
    let second_boundary = serde_json::json!({
        "groupID": 1700000000002_i64, "date": "2025-03-02T10:00:00.000Z", "chunks": ["Second"], "url": v1::ESSAY_URL,
    })
    .to_string();
    let deleted_boundary = serde_json::json!({
        "groupID": 1700000000003_i64, "date": "2025-03-03T10:00:00.000Z", "chunks": ["Gone"], "url": v1::ESSAY_URL,
    })
    .to_string();
    let other_boundary = serde_json::json!({
        "groupID": 1600000000000_i64, "date": "2023-05-01T00:00:00.000Z", "chunks": ["Old"], "url": v1::OTHER_URL,
    })
    .to_string();
    let research_boundary = |page: i64| {
        serde_json::json!({ "pageNumber": page, "position": research_position(page), "source": "research" })
            .to_string()
    };

    let annotation = "INSERT INTO annotations
        (id, resource_id, text, color, boundary, external_id, deleted_at, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, '2025-03-05T00:00:00.000Z', '2025-03-06T00:00:00.000Z')";
    for (id, resource, text, color, boundary, external_id, deleted) in [
        (
            1,
            1,
            v1::ESSAY_TEXT,
            "yellow",
            essay_boundary,
            "light_aaa:1700000000001",
            None,
        ),
        (
            2,
            1,
            "Second",
            "yellow",
            second_boundary,
            "light_bbb:1700000000002",
            None,
        ),
        (
            3,
            1,
            "Gone",
            "yellow",
            deleted_boundary,
            "light_aaa:1700000000003",
            Some("2025-03-04T00:00:00.000Z"),
        ),
        (
            4,
            2,
            "Old",
            "yellow",
            other_boundary,
            "light_ccc:1600000000000",
            None,
        ),
        (
            5,
            3,
            "Language is a carrier of culture",
            "#ffd400",
            research_boundary(3),
            "research:ann-1",
            None,
        ),
        (
            6,
            3,
            "The bullet was the means",
            "#ffd400",
            research_boundary(12),
            "research:ann-2",
            None,
        ),
        (
            7,
            6,
            "On a deleted page",
            "yellow",
            "{}".to_string(),
            "light_aaa:1700000000009",
            None,
        ),
    ] {
        db.run(
            annotation,
            libsql::params![id, resource, text, color, boundary, external_id, deleted],
        )
        .await;
    }

    run(
        "INSERT INTO comments (id, annotation_id, content, external_id, deleted_at) VALUES
         (1, 1, 'against forgetting', 'light_aaa:1700000000001:c1', NULL),
         (2, 3, 'on a removed highlight', 'light_aaa:1700000000003:c2', NULL),
         (3, 5, 'Ngũgĩ on language', 'research:com-1', NULL),
         (4, 5, 'written by hand in Bibliotek', NULL, NULL)",
    )
    .await;
    run(
        "INSERT INTO notes (id, resource_id, content, external_id, deleted_at) VALUES
         (1, 3, 'A long Research note', 'research:note-1', NULL),
         (2, 3, 'A note written in Bibliotek', NULL, NULL),
         (3, 3, 'A note Research deleted', 'research:note-2', '2025-04-01T00:00:00.000Z')",
    )
    .await;

    run("INSERT INTO reader_settings (id, target_language) VALUES (1, 'fr')").await;
    run("INSERT INTO research_config (id, db_path) VALUES (1, '/Users/someone/Research/research.db')").await;
}

pub async fn research_matching_standard_v1(db: &ResearchDb) {
    db.item("item-1", v1::RESEARCH_TITLE).await;
    db.item("item-2", "Some Paper").await;
    db.annotation("ann-1", "item-1", "Language is a carrier of culture", 3)
        .await;
    db.annotation("ann-2", "item-1", "The bullet was the means", 12)
        .await;
    db.comment("com-1", "ann-1", "Ngũgĩ on language").await;
    db.note("note-1", "item-1", "A long Research note").await;
}
