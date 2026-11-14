mod common;

use std::fs;
use std::path::Path;

use axum::http::{Method, StatusCode, header};
use serde_json::json;

use common::TestApp;

const QUINE_URL: &str = "http://liotek.test/reader/quine-two-dogmas/book/book.html";

fn write(root: &Path, path: &str, contents: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn books_in(root: &Path) {
    write(root, "reader/fit.js", "/* fit */");
    write(root, "reader/fonts/Spectral.woff2", "font");
    write(
        root,
        "quine-two-dogmas/book.toml",
        "title = \"Two Dogmas of Empiricism\"\nauthor = \"W. V. O. Quine\"\ntranslator = \"\"\nyear = 1951\n",
    );
    write(
        root,
        "quine-two-dogmas/book/book.html",
        "<html><head></head><body>Quine</body></html>",
    );
    write(root, "quine-two-dogmas/source.pdf", "%PDF scan");
    write(root, "quine-two-dogmas/work/report.json", "{}");
    write(
        root,
        "bergson-time-free-will/book.toml",
        "title = \"Time and Free Will\"\nauthor = \"Henri Bergson\"\ntranslator = \"F. L. Pogson\"\nyear = \"1913\"\n",
    );
    write(
        root,
        "bergson-time-free-will/book/book.html",
        "<html>Bergson</html>",
    );
    write(
        root,
        "whitehead-process/book.toml",
        "title = \"Process and Reality\"\nauthor = \"Alfred North Whitehead\"\nyear = \"1929\"\n",
    );
    write(root, "whitehead-process/book/book.html", "<html>Whitehead</html>");
    write(
        root,
        "unbuilt/book.toml",
        "title = \"Not Yet\"\nauthor = \"Someone\"\n",
    );
    write(root, "broken/book.toml", "title = ");
    write(root, "broken/book/book.html", "<html>Broken</html>");
}

async fn reader_app() -> TestApp {
    TestApp::start_configured(|config, dir| {
        let books = dir.join("books");
        books_in(&books);
        write(dir, "outside.txt", "not a book");
        config.reader_books_dir = Some(books);
    })
    .await
}

#[tokio::test]
async fn reader_routes_are_absent_without_a_reader_config() {
    let app = TestApp::start().await;

    assert_eq!(app.get("/api/reader/books").await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        app.get("/reader/reader/fit.js").await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_book_page_and_the_shared_reader_files_are_served() {
    let app = reader_app().await;

    let page = app.get("/reader/quine-two-dogmas/book/book.html").await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(
        page.headers[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert_eq!(&page.bytes[..], b"<html><head></head><body>Quine</body></html>");

    let script = app.get("/reader/reader/fit.js").await;
    assert_eq!(script.status, StatusCode::OK);
    assert_eq!(&script.bytes[..], b"/* fit */");

    let font = app.get("/reader/reader/fonts/Spectral.woff2").await;
    assert_eq!(font.status, StatusCode::OK);
}

#[tokio::test]
async fn source_pdfs_work_files_and_book_toml_are_never_served() {
    let app = reader_app().await;

    for path in [
        "/reader/quine-two-dogmas/source.pdf",
        "/reader/quine-two-dogmas/work/report.json",
        "/reader/quine-two-dogmas/book.toml",
        "/reader/quine-two-dogmas/book",
        "/reader/quine-two-dogmas",
        "/reader/reader",
    ] {
        assert_eq!(app.get(path).await.status, StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn paths_that_leave_the_books_folder_are_refused() {
    let app = reader_app().await;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        app.dir.path().join("outside.txt"),
        app.dir.path().join("books/reader/escape.txt"),
    )
    .unwrap();

    for path in [
        "/reader/reader/../../outside.txt",
        "/reader/reader/%2E%2E/%2E%2E/outside.txt",
        "/reader/reader/..%2F..%2Foutside.txt",
        "/reader/reader/escape.txt",
    ] {
        assert_eq!(app.get(path).await.status, StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn the_catalog_lists_only_built_books_by_surname() {
    let app = reader_app().await;

    let books = app.get("/api/reader/books").await.data();

    assert_eq!(
        books,
        json!([
            {
                "slug": "bergson-time-free-will",
                "title": "Time and Free Will",
                "author": "Henri Bergson",
                "translator": "F. L. Pogson",
                "publisher": "",
                "year": "1913",
                "url": "/reader/bergson-time-free-will/book/book.html",
                "highlights": 0,
            },
            {
                "slug": "quine-two-dogmas",
                "title": "Two Dogmas of Empiricism",
                "author": "W. V. O. Quine",
                "translator": "",
                "publisher": "",
                "year": "1951",
                "url": "/reader/quine-two-dogmas/book/book.html",
                "highlights": 0,
            },
            {
                "slug": "whitehead-process",
                "title": "Process and Reality",
                "author": "Alfred North Whitehead",
                "translator": "",
                "publisher": "",
                "year": "1929",
                "url": "/reader/whitehead-process/book/book.html",
                "highlights": 0,
            },
        ])
    );
}

#[tokio::test]
async fn the_catalog_counts_highlights_on_each_books_reader_url() {
    let app = reader_app().await;
    let highlight = |group_id: i64, text: &str, url: &str| {
        json!({ "groupID": group_id, "repr": text, "chunks": [text], "url": url,
                "date": "2026-11-14T09:00:00.000Z" })
    };
    let synced = app
        .admin(
            Method::PUT,
            "/api/imports/light/aaa",
            Some(json!({ "highlights": {
                QUINE_URL: [
                    highlight(1, "two dogmas", QUINE_URL),
                    highlight(2, "analytic", QUINE_URL),
                ],
                "https://elsewhere.example/quine-two-dogmas": [
                    highlight(3, "unrelated", "https://elsewhere.example/quine-two-dogmas"),
                ],
            } })),
        )
        .await;
    assert_eq!(synced.status, StatusCode::OK, "{}", synced.json());

    let books = app.get("/api/reader/books").await.data();

    assert_eq!(books[0]["slug"], "bergson-time-free-will");
    assert_eq!(books[0]["highlights"], 0);
    assert_eq!(books[1]["slug"], "quine-two-dogmas");
    assert_eq!(books[1]["highlights"], 2);
}
