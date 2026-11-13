# liotek

A personal library daemon: documents, highlights, comments, and notes in one local
SQLite file, with PDFs in an S3-compatible store.

## Develop

```bash
make dev        # daemon on 127.0.0.1:6579, data in .dev/, admin token dev-admin-token
make dev-web    # frontend with hot reload
make check      # format, lint, all tests, build
```

## Run

```bash
cp config.example.yaml ~/.liotek/config.yaml   # then edit
make release
liotek doctor --config ~/.liotek/config.yaml
make install                                   # launchd agent
```

Commands, backups, environments, and the rules for running beside Bibliotek are in
[ctx.md](ctx.md).
