#!/usr/bin/env bash
# Fetch the latest CI diagnostics for the working branch through the GitHub
# checks API (annotations), since raw job logs are not reachable from here.
set -u
BRANCH=arena/01a024b4-ferrumix
run=$(gh run list -b "$BRANCH" -L 1 --json databaseId,status,conclusion -q '.[0].databaseId')
status=$(gh run list -b "$BRANCH" -L 1 --json status,conclusion -q '.[0].status + " " + (.[0].conclusion // "-")')
echo "run $run: $status"
for j in $(gh api "repos/kostbog/ferrumix/actions/runs/$run/jobs" -q '.jobs[] | select(.name=="make test") | .id'); do
  gh api "repos/kostbog/ferrumix/check-runs/$j/annotations" -q '.[] | .message'
done | python3 -c "import sys; sys.stdout.write(sys.stdin.read().replace('%0A','\n').replace('%0D','').replace('%25','%'))"
