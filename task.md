I want you to refactor and redesign the web/index.html file.
My style and taste in products is minimalist and functional, your refactors should reflect that.

The purpose of that page is to allow users upload books(as you will see with the existing code) and to allow them view all uploaded books, in a paginated manner.

There should be a search functionality that calls the /books with the query params in api.rs.

There should also be an aggregated tree view on the left-hand-side that aggregates books by metadata (authors(23), tags(5), ratings(4)). And when a user clicks a metadata, for eg. author, a sub-dropdown should show all authors, and aggregates of their books. Use lines/outlines to replicate the tree view.

Implement a backend api to display all the aggregates, you might have to update the schema to add more metadata fields.
