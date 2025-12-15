-- Parent categories
INSERT OR IGNORE INTO categories (name, parent_id, description) VALUES
('formal', NULL, 'Formal sciences including mathematics, logic, and statistics'),
('natural science', NULL, 'Natural sciences including physics, chemistry, biology, etc.'),
('social science', NULL, 'Social sciences including psychology, sociology, economics, etc.'),
('humanities', NULL, 'Humanities including history, philosophy, literature, etc.'),
('applied', NULL, 'Applied sciences including engineering, technology, business, etc.'),
('interdisciplinary', NULL, 'Interdisciplinary fields combining multiple domains');

-- Formal science children
INSERT OR IGNORE INTO categories (name, parent_id, description) VALUES
('mathematics', (SELECT id FROM categories WHERE name = 'formal'), NULL),
('logic', (SELECT id FROM categories WHERE name = 'formal'), NULL),
('statistics', (SELECT id FROM categories WHERE name = 'formal'), NULL);

-- Natural science children
INSERT OR IGNORE INTO categories (name, parent_id, description) VALUES
('physics', (SELECT id FROM categories WHERE name = 'natural science'), NULL),
('chemistry', (SELECT id FROM categories WHERE name = 'natural science'), NULL),
('astronomy', (SELECT id FROM categories WHERE name = 'natural science'), NULL),
('earth', (SELECT id FROM categories WHERE name = 'natural science'), NULL),
('biology', (SELECT id FROM categories WHERE name = 'natural science'), NULL),
('medicine', (SELECT id FROM categories WHERE name = 'natural science'), NULL),
('environment', (SELECT id FROM categories WHERE name = 'natural science'), NULL);

-- Social science children
INSERT OR IGNORE INTO categories (name, parent_id, description) VALUES
('psychology', (SELECT id FROM categories WHERE name = 'social science'), NULL),
('sociology', (SELECT id FROM categories WHERE name = 'social science'), NULL),
('anthropology', (SELECT id FROM categories WHERE name = 'social science'), NULL),
('economics', (SELECT id FROM categories WHERE name = 'social science'), NULL),
('politics', (SELECT id FROM categories WHERE name = 'social science'), NULL),
('law', (SELECT id FROM categories WHERE name = 'social science'), NULL);

-- Humanities children
INSERT OR IGNORE INTO categories (name, parent_id, description) VALUES
('history', (SELECT id FROM categories WHERE name = 'humanities'), NULL),
('philosophy', (SELECT id FROM categories WHERE name = 'humanities'), NULL),
('literature', (SELECT id FROM categories WHERE name = 'humanities'), NULL),
('religion', (SELECT id FROM categories WHERE name = 'humanities'), NULL),
('art', (SELECT id FROM categories WHERE name = 'humanities'), NULL),
('music', (SELECT id FROM categories WHERE name = 'humanities'), NULL),
('communication', (SELECT id FROM categories WHERE name = 'humanities'), NULL);

-- Applied science children
INSERT OR IGNORE INTO categories (name, parent_id, description) VALUES
('engineering', (SELECT id FROM categories WHERE name = 'applied'), NULL),
('architecture', (SELECT id FROM categories WHERE name = 'applied'), NULL),
('technology', (SELECT id FROM categories WHERE name = 'applied'), NULL),
('manufacturing', (SELECT id FROM categories WHERE name = 'applied'), NULL),
('transportation', (SELECT id FROM categories WHERE name = 'applied'), NULL),
('business', (SELECT id FROM categories WHERE name = 'applied'), NULL),
('finance', (SELECT id FROM categories WHERE name = 'applied'), NULL),
('insurance', (SELECT id FROM categories WHERE name = 'applied'), NULL),
('education', (SELECT id FROM categories WHERE name = 'applied'), NULL);

-- Interdisciplinary children
INSERT OR IGNORE INTO categories (name, parent_id, description) VALUES
('computers', (SELECT id FROM categories WHERE name = 'interdisciplinary'), NULL),
('artificial intelligence', (SELECT id FROM categories WHERE name = 'interdisciplinary'), NULL),
('biotechnology', (SELECT id FROM categories WHERE name = 'interdisciplinary'), NULL),
('nanotechnology', (SELECT id FROM categories WHERE name = 'interdisciplinary'), NULL);