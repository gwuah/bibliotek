#!/bin/bash
set -e

echo "Running database migrations..."
cargo run

echo "Migrations completed successfully!"