For large migrations, you can use rust_embed;

```rust
   const MIGRATION_001: &str = include_str!("migrations/001_initial.sql");
   const MIGRATION_002: &str = include_str!("migrations/002_add_indexes.sql");
```

```rust
   use rust_embed::RustEmbed;

   #[derive(RustEmbed)]
   #[folder = "migrations/"]
   struct Migrations;
```
