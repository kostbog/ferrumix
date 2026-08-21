#!/usr/bin/env bash
# TEMPORARY: reproduce the nightly custom-target build and surface its output
# as GitHub Actions annotations (raw job logs are not reachable from the
# development sandbox).  Never fails the build.
set -u
log=$(mktemp)
{
  rustup toolchain install nightly --profile minimal -c rust-src 2>&1 | tail -2
  cargo +nightly build --target x86_64-ferrumix.json \
    -Zbuild-std=core,compiler_builtins --message-format short 2>&1 | tail -25
  echo "custom target exit: $?"
} >"$log" 2>&1
sed -i -e 's/\x1b\[[0-9;]*m//g' "$log"
cat "$log"
if [ -n "${GITHUB_ACTIONS:-}" ]; then
  msg=$(python3 -c "
import sys
print(open(sys.argv[1], errors='replace').read()[-8000:].replace('%','%25').replace('\r','%0D').replace('\n','%0A'))
" "$log")
  echo "::warning::${msg}"
fi
exit 0
