#!/usr/bin/env bash
set -euo pipefail

cargo install --path . --force

echo "Ferrum installed. Run it with: ferrum"
