#!/usr/bin/env bash
# ci-diag.sh — TEMPORARY development helper.
#
# Runs the full local verification pipeline (fmt, build, clippy, QEMU boot
# test) and, when running inside GitHub Actions, echoes the tail of the log
# back as workflow annotations (`::error::` / `::notice::`) so the result is
# visible through the checks API without downloading raw job logs.
#
# This file is a scaffolding aid for developing without a local Rust
# toolchain; it is not part of the kernel.

set -u

TARGET="${TARGET:-x86_64-unknown-none}"
KERNEL="target/${TARGET}/debug/ferrumix"
LOG="$(mktemp -t ferrumix-ci.XXXXXX.log)"
status=0

{
  echo "== toolchain"
  rustc -V
  cargo -V
  echo

  echo "== cargo fmt --all --check (files needing format)"
  cargo fmt --all --check > /tmp/fmt.out 2>&1
  fmt_rc=$?
  grep -E "^Diff in" /tmp/fmt.out | sed "s|/home/runner/work/ferrumix/ferrumix/||" | sort -u | head -60
  echo "fmt exit: ${fmt_rc}"
  echo

  echo "== cargo build --target ${TARGET}"
  cargo build --target "${TARGET}" --message-format short > /tmp/build.out 2>&1
  build_rc=$?
  grep -E "error|warning" /tmp/build.out | head -30
  if [ "${build_rc}" -ne 0 ]; then
    echo "-- verbose rebuild --"
    cargo build --target "${TARGET}" 2>&1 | grep -v "^ *= note: *$" | tail -45
  fi
  echo "build exit: ${build_rc}"
  echo

  if [ "${build_rc}" -eq 0 ]; then
    echo "== cargo clippy"
    cargo clippy --target "${TARGET}" --message-format short 2>&1 |
      grep -E "warning|error" | sed "s/^.*: //" | sort | uniq -c | sort -rn | head -15
    echo

    echo "== image layout"
    nm "${KERNEL}" 2>/dev/null | grep -E "__kernel_(start|end)|__bss_(start|end)" | sort
    readelf -l "${KERNEL}" 2>/dev/null | grep -E "LOAD|NOTE" | head -8
    echo

    echo "== boot test"
    timeout 25 qemu-system-x86_64 -kernel "${KERNEL}" -serial stdio -display none -monitor none
    echo "qemu exit: $?"
  fi
} 2>&1 | sed -e 's/\x1b\[[0-9;]*m//g' >"${LOG}"

# Decide pass/fail the same way `make test` does.
if ! grep -q "^fmt exit: 0$" "${LOG}"; then status=10; fi
if ! grep -q "^build exit: 0$" "${LOG}"; then status=20; fi
if [ "${status}" -eq 0 ]; then
  for pat in "Ferrumix 0.1.0" "is alive" "ferrumix>" "paging: self-test OK" \
             "syscall: self-test" "elf: hello loaded" "hello from ring 3" \
             "exited with status 7"; do
    grep -q "${pat}" "${LOG}" || status=30
  done
fi

cat "${LOG}"

if [ -n "${GITHUB_ACTIONS:-}" ]; then
  level="notice"
  [ "${status}" -ne 0 ] && level="error"
  chunkdir="$(mktemp -d)"
  tail -c 24000 "${LOG}" | split -l 25 - "${chunkdir}/chunk_"
  for f in "${chunkdir}"/chunk_*; do
    msg="$(python3 -c "
import sys
data = open(sys.argv[1], 'r', errors='replace').read()
print(data.replace('%', '%25').replace('\r', '%0D').replace('\n', '%0A'))
" "$f")"
    echo "::${level}::${msg}"
  done
fi

exit "${status}"
