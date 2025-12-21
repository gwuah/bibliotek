use axum::{
    Router,
    routing::{delete, get, post, put},
};

use super::handler;
use crate::handler::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/resources", get(handler::list_resources))
        .route("/resources", post(handler::create_resource))
        .route("/resources/:id", get(handler::get_resource))
        .route("/resources/:id", put(handler::update_resource))
        .route("/resources/:id", delete(handler::delete_resource))
        .route("/resources/:id/full", get(handler::get_resource_full))
        .route(
            "/resources/:id/annotations",
            get(handler::list_annotations_by_resource),
        )
        .route("/resources/:id/notes", get(handler::list_notes_by_resource))
        .route("/resources/:id/words", get(handler::list_words_by_resource))
        .route("/annotations", post(handler::create_annotation))
        .route("/annotations/:id", get(handler::get_annotation))
        .route("/annotations/:id", put(handler::update_annotation))
        .route("/annotations/:id", delete(handler::delete_annotation))
        .route(
            "/annotations/:id/comments",
            get(handler::list_comments_by_annotation),
        )
        .route("/comments", post(handler::create_comment))
        .route("/comments/:id", get(handler::get_comment))
        .route("/comments/:id", put(handler::update_comment))
        .route("/comments/:id", delete(handler::delete_comment))
        .route("/notes", post(handler::create_note))
        .route("/notes/:id", get(handler::get_note))
        .route("/notes/:id", put(handler::update_note))
        .route("/notes/:id", delete(handler::delete_note))
        .route("/words", post(handler::create_word))
        .route("/words", get(handler::search_words))
        .route("/words/:id", get(handler::get_word))
        .route("/words/:id", put(handler::update_word))
        .route("/words/:id", delete(handler::delete_word))
}
