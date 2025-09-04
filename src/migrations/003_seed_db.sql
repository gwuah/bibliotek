-- Seed data for UI testing
-- Authors
INSERT
    OR IGNORE INTO authors (name)
VALUES
    ('Isaac Newton'),
    ('Albert Einstein'),
    ('Charles Darwin'),
    ('Aristotle'),
    ('Marie Curie'),
    ('Stephen Hawking'),
    ('Alan Turing'),
    ('Virginia Woolf'),
    ('William Shakespeare'),
    ('Plato'),
    ('Leonardo da Vinci'),
    ('Nikola Tesla'),
    ('Ada Lovelace'),
    ('Jane Austen'),
    ('Ernest Hemingway');

-- Tags
INSERT
    OR IGNORE INTO tags (name)
VALUES
    ('classic'),
    ('biography'),
    ('textbook'),
    ('reference'),
    ('popular science'),
    ('fiction'),
    ('non-fiction'),
    ('illustrated'),
    ('award winner'),
    ('bestseller');

-- Books
INSERT
    OR IGNORE INTO books (
        title,
        url,
        cover_url,
        ratings,
        description,
        pages
    )
VALUES
    (
        'Principia Mathematica',
        'https://example.com/books/principia.pdf',
        'https://example.com/covers/principia.jpg',
        5,
        'A mathematical treatise on the foundations of mathematics',
        100
    ),
    (
        'The Origin of Species',
        'https://example.com/books/origin.pdf',
        'https://example.com/covers/origin.jpg',
        5,
        'A scientific treatise on the origin of species',
        450
    ),
    (
        'A Brief History of Time',
        'https://example.com/books/brief-history.pdf',
        'https://example.com/covers/brief-history.jpg',
        4,
        'A scientific treatise on the history of time',
        300
    ),
    (
        'Computing Machinery and Intelligence',
        'https://example.com/books/computing.pdf',
        'https://example.com/covers/computing.jpg',
        5,
        'A scientific treatise on the intelligence of computers',
        200
    ),
    (
        'Mrs. Dalloway',
        'https://example.com/books/dalloway.pdf',
        'https://example.com/covers/dalloway.jpg',
        4,
        'A novel about a woman and her thoughts',
        128
    ),
    (
        'Hamlet',
        'https://example.com/books/hamlet.pdf',
        'https://example.com/covers/hamlet.jpg',
        5,
        'A play about a man and his thoughts',
        324
    ),
    (
        'The Republic',
        'https://example.com/books/republic.pdf',
        'https://example.com/covers/republic.jpg',
        5,
        'A philosophical treatise on the nature of the republic',
        100
    ),
    (
        'Notebooks',
        'https://example.com/books/notebooks.pdf',
        'https://example.com/covers/notebooks.jpg',
        4,
        'A notebook about a man and his thoughts',
        600
    ),
    (
        'My Inventions',
        'https://example.com/books/inventions.pdf',
        'https://example.com/covers/inventions.jpg',
        4,
        'A notebook about a man and his inventions',
        443
    ),
    (
        'Notes on the First Mechanical Computer',
        'https://example.com/books/notes.pdf',
        'https://example.com/covers/notes.jpg',
        4,
        'A notebook about a man and his thoughts',
        388
    ),
    (
        'Pride and Prejudice',
        'https://example.com/books/pride.pdf',
        'https://example.com/covers/pride.jpg',
        4,
        'A novel about a man and his thoughts',
        499
    ),
    (
        'The Sun Also Rises',
        'https://example.com/books/sun.pdf',
        'https://example.com/covers/sun.jpg',
        4,
        'A novel about a man and his thoughts',
        277
    ),
    (
        'Relativity: The Special and General Theory',
        'https://example.com/books/relativity.pdf',
        'https://example.com/covers/relativity.jpg',
        5,
        'A scientific treatise on the relativity',
        299
    ),
    (
        'Radium and Other Radioactive Substances',
        'https://example.com/books/radium.pdf',
        'https://example.com/covers/radium.jpg',
        4,
        'A scientific treatise on the radioactive substances',
        311
    ),
    (
        'On the Motion of the Heart and Blood',
        'https://example.com/books/heart.pdf',
        'https://example.com/covers/heart.jpg',
        4,
        'A scientific treatise on the motion of the heart and blood',
        324
    );

-- Book-Author relationships
INSERT
    OR IGNORE INTO book_authors (book_id, author_id)
VALUES
    (1, 1),
    -- Newton - Principia
    (2, 3),
    -- Darwin - Origin
    (3, 6),
    -- Hawking - Brief History
    (4, 7),
    -- Turing - Computing
    (5, 8),
    -- Woolf - Mrs Dalloway
    (6, 9),
    -- Shakespeare - Hamlet
    (7, 10),
    -- Plato - Republic
    (8, 11),
    -- da Vinci - Notebooks
    (9, 12),
    -- Tesla - Inventions
    (10, 13),
    -- Lovelace - Notes
    (11, 14),
    -- Austen - Pride
    (12, 15),
    -- Hemingway - Sun
    (13, 2),
    -- Einstein - Relativity
    (14, 5),
    -- Curie - Radium
    (15, 3);

-- Darwin - Heart (example)
-- Book-Category relationships
INSERT
    OR IGNORE INTO book_categories (book_id, category_id)
VALUES
    -- Mathematics books
    (
        1,
        (
            SELECT
                id
            FROM
                categories
            WHERE
                name = 'mathematics'
        )
    ),
    -- Principia
    (
        10,
        (
            SELECT
                id
            FROM
                categories
            WHERE
                name = 'mathematics'
        )
    ),
    -- Lovelace Notes
    -- Physics books
    (
        3,
        (
            SELECT
                id
            FROM
                categories
            WHERE
                name = 'physics'
        )
    ),
    -- Brief History
    (
        13,
        (
            SELECT
                id
            FROM
                categories
            WHERE
                name = 'physics'
        )
    ),
    -- Relativity
    (
        14,
        (
            SELECT
                id
            FROM
                categories
            WHERE
                name = 'physics'
        )
    ),
    -- Radium
    -- Biology books
    (
        2,
        (
            SELECT
                id
            FROM
                categories
            WHERE
                name = 'biology'
        )
    ),
    -- Origin
    (
        15,
        (
            SELECT
                id
            FROM
                categories
            WHERE
                name = 'medicine'
        )
    ),
    -- Heart
    -- Technology/Computer books
    (
        4,
        (
            SELECT
                id
            FROM
                categories
            WHERE
                name = 'computers'
        )
    ),
    -- Computing
    (
        9,
        (
            SELECT
                id
            FROM
                categories
            WHERE
                name = 'technology'
        )
    ),
    -- Tesla
    -- Literature books
    (
        5,
        (
            SELECT
                id
            FROM
                categories
            WHERE
                name = 'literature'
        )
    ),
    -- Mrs Dalloway
    (
        6,
        (
            SELECT
                id
            FROM
                categories
            WHERE
                name = 'literature'
        )
    ),
    -- Hamlet
    (
        11,
        (
            SELECT
                id
            FROM
                categories
            WHERE
                name = 'literature'
        )
    ),
    -- Pride
    (
        12,
        (
            SELECT
                id
            FROM
                categories
            WHERE
                name = 'literature'
        )
    ),
    -- Sun
    -- Philosophy books
    (
        7,
        (
            SELECT
                id
            FROM
                categories
            WHERE
                name = 'philosophy'
        )
    ),
    -- Republic
    -- Art books
    (
        8,
        (
            SELECT
                id
            FROM
                categories
            WHERE
                name = 'art'
        )
    );

-- da Vinci
-- Book-Tag relationships
INSERT
    OR IGNORE INTO book_tags (book_id, tag_id)
VALUES
    -- Classic tags
    (
        1,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'classic'
        )
    ),
    (
        2,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'classic'
        )
    ),
    (
        6,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'classic'
        )
    ),
    (
        7,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'classic'
        )
    ),
    (
        11,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'classic'
        )
    ),
    -- Popular science tags
    (
        3,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'popular science'
        )
    ),
    (
        13,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'popular science'
        )
    ),
    -- Biography tags
    (
        9,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'biography'
        )
    ),
    -- Fiction tags
    (
        5,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'fiction'
        )
    ),
    (
        6,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'fiction'
        )
    ),
    (
        11,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'fiction'
        )
    ),
    (
        12,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'fiction'
        )
    ),
    -- Non-fiction tags
    (
        1,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'non-fiction'
        )
    ),
    (
        2,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'non-fiction'
        )
    ),
    (
        3,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'non-fiction'
        )
    ),
    (
        4,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'non-fiction'
        )
    ),
    (
        7,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'non-fiction'
        )
    ),
    (
        8,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'non-fiction'
        )
    ),
    (
        9,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'non-fiction'
        )
    ),
    (
        10,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'non-fiction'
        )
    ),
    (
        13,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'non-fiction'
        )
    ),
    (
        14,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'non-fiction'
        )
    ),
    (
        15,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'non-fiction'
        )
    ),
    -- Award winner tags
    (
        2,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'award winner'
        )
    ),
    (
        3,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'award winner'
        )
    ),
    (
        11,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'award winner'
        )
    ),
    -- Bestseller tags
    (
        3,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'bestseller'
        )
    ),
    (
        11,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'bestseller'
        )
    ),
    (
        12,
        (
            SELECT
                id
            FROM
                tags
            WHERE
                name = 'bestseller'
        )
    );