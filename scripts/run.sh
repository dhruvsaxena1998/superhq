#!/bin/bash
set -e
cargo build
codesign --sign - --entitlements entitlements.plist --force target/debug/superhq
exec ./target/debug/superhq "$@"
