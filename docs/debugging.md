cargo clean && cargo run --bin bibliotek -- -c config.yaml
rm -rf target/debug/incremental && cargo run --bin bibliotek -- -c config.yaml
