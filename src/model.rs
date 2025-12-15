use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Book {
    pub id: i32,
    pub title: String,
    pub download_url: String,
    pub cover_url: String,
    pub ratings: i32,
    pub author_ids: Vec<String>,
    pub tag_ids: Vec<String>,
    pub category_ids: Vec<String>,
    pub description: String,
    pub pages: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Author {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Tag {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Category {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Rating {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthorAggregate {
    pub author: Author,
    pub count: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TagAggregate {
    pub tag: Tag,
    pub count: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RatingAggregate {
    pub rating: Rating,
    pub count: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryAggregate {
    pub category: Category,
    pub count: i32,
}
