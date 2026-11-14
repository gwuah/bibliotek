const folded = (text) =>
  (text ?? "")
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();

export function shelfOf(books, query = "") {
  const words = folded(query).split(/\s+/).filter(Boolean);
  const matches = (book) => {
    const haystack = folded(`${book.title} ${book.author} ${book.translator}`);
    return words.every((word) => haystack.includes(word));
  };

  const shelves = new Map();
  for (const book of books.filter(matches)) {
    const author = book.author || "Unknown author";
    if (!shelves.has(author)) shelves.set(author, []);
    shelves.get(author).push(book);
  }
  return [...shelves].map(([author, books]) => ({ author, books }));
}
