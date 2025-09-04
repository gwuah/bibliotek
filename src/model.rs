use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Book {
    pub id: i32,
    pub title: String,
    pub download_url: String,
    pub cover_url: String,
    pub ratings: Option<i32>,
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

// match row.get(0)? {
//                 "author" => author_aggregates.push(AuthorAggregate {
//                     author: Author {
//                         id: row.get(1)?,
//                         name: row.get(2)?,
//                     },
//                     book_count: row.get(3)?,
//                 }),
//                 "category" => category_aggregates.push(CategoryAggregate {
//                     category: Category {
//                         id: row.get(1)?,
//                         name: row.get(2)?,
//                     },
//                     book_count: row.get(3)?,
//                 }),
//                 "tag" => tag_aggregates.push(TagAggregate {
//                     tag: Tag {
//                         id: row.get(1)?,
//                         name: row.get(2)?,
//                     },
//                     book_count: row.get(3)?,
//                 }),
//             }
