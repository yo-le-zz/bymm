#!/bin/sh
# Builds the fake executable, then runs every test program against copies of it.
set -e
cd "$(dirname "$0")"
cargo run -q -p lib_test --bin setup
for t in write append read delete clear limit run invalid_file; do
  cargo run -q -p lib_test --bin "test_$t"
done
echo "ALL OK"
