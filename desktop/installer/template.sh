#!/usr/bin/env sh
# Keeps `installer.nsi`, our copy of Tauri's NSIS template, in step with Tauri.
#
#   template.sh check          fails when the Tauri CLI in use is not the one the copy came from
#   template.sh diff           our changes: upstream (the version below) against our copy
#   template.sh merge VERSION  merges upstream's changes from the version below to tauri-bundler
#                              VERSION into our copy (three-way, conflict markers where both
#                              changed the same lines); then update the versions here and in
#                              the copy's header
#
# Run it from anywhere; `diff` and `merge` download the templates from GitHub.
set -eu

# The tauri-bundler our copy came from, and the @tauri-apps/cli that bundles it.
BUNDLER=2.10.4
CLI=2.12.4

here=$(cd "$(dirname "$0")" && pwd)
ours="$here/installer.nsi"

upstream() {
  url="https://raw.githubusercontent.com/tauri-apps/tauri/tauri-bundler-v$1/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi"
  curl --fail --silent --show-error --location --output "$2" "$url"
}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

case "${1:-}" in
  check)
    used=$(pnpm --silent --dir "$here/.." exec tauri --version)
    if [ "$used" != "tauri-cli $CLI" ]; then
      echo "installer.nsi was copied for tauri-cli $CLI (tauri-bundler $BUNDLER), the build uses $used:" >&2
      echo "find its tauri-bundler version and run: $0 merge <version>" >&2
      exit 1
    fi
    echo "installer.nsi matches $used (tauri-bundler $BUNDLER)"
    ;;
  diff)
    upstream "$BUNDLER" "$work/upstream.nsi"
    diff -u --label "tauri-bundler $BUNDLER" --label "installer.nsi" "$work/upstream.nsi" "$ours" || true
    ;;
  merge)
    new=${2:?usage: $0 merge VERSION}
    upstream "$BUNDLER" "$work/base.nsi"
    upstream "$new" "$work/new.nsi"
    if git merge-file -L installer.nsi -L "tauri-bundler $BUNDLER" -L "tauri-bundler $new" \
      "$ours" "$work/base.nsi" "$work/new.nsi"; then
      echo "Merged tauri-bundler $new without conflicts."
    else
      echo "Merged tauri-bundler $new: resolve the conflict markers in $ours." >&2
    fi
    echo "Then set BUNDLER and CLI in $0 and the versions in the copy's header." >&2
    ;;
  *)
    sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 2
    ;;
esac
