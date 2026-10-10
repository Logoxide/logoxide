#!/usr/bin/env bash
# For CI, until topos 0.6 and sword-modules 0.1 are on crates.io: checks crates.io for the
# versions logoxide depends on, and for each that isn't there, clones its repository and adds a
# [patch.crates-io] for it to <folder>/.cargo/config.toml (never committed: `/.cargo/` is
# ignored). Once both are published it does nothing, and it can be removed.
#
#   scripts/patch-unpublished.sh <folder> [topos] [sword-modules]
#
# topos comes from github.com/MasterTemple/topos-bible at `scripts/topos-ref.sh`'s tag (else its
# default branch, with a warning). sword-modules has no public repository yet: set the
# SWORD_MODULES_REPO variable (a git URL; SWORD_MODULES_REF for a branch or tag, else
# v<version>) to build against one, or publish it. The bindings patch topos in their own
# Cargo.toml, so for them pass only `sword-modules`.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
folder=$1
shift
which=("$@")
if [[ ${#which[@]} -eq 0 ]]; then
  which=(topos sword-modules)
fi
deps=${RUNNER_TEMP:-${TMPDIR:-/tmp}}/logoxide-unpublished
mkdir -p "$deps" "$folder/.cargo"
config="$folder/.cargo/config.toml"

# Whether crates.io has this version of a crate
published() {
  local code
  code=$(curl -s -o /dev/null -w '%{http_code}' -A 'logoxide-ci (github.com/Logoxide/logoxide)' \
    "https://crates.io/api/v1/crates/$1/$2")
  [[ "$code" == 200 ]]
}

# The version the workspace's Cargo.toml asks for (`name = "1.0"` or `name = { version = "1.0", … }`)
version_of() {
  sed -n "s/^$1 = \(\"\|{ *version = \"\)\([0-9][^\"]*\)\".*/\2/p" "$root/Cargo.toml" | head -1
}

# A shallow clone at a tag or branch, else the default branch
clone() {
  if ! git clone -q --depth 1 --branch "$2" "$1" "$3" 2>/dev/null; then
    echo "::warning::$1 has no $2; building against its default branch"
    rm -rf "$3"
    git clone -q --depth 1 "$1" "$3"
  fi
}

patches=()
for name in "${which[@]}"; do
  case $name in
    topos)
      version=$(version_of topos-bible)
      if published topos-bible "$version"; then
        echo "topos-bible $version is on crates.io"
        continue
      fi
      echo "topos-bible $version isn't on crates.io: building against a checkout"
      if [[ ! -d "$deps/topos" ]]; then
        clone https://github.com/MasterTemple/topos-bible "$("$root/scripts/topos-ref.sh")" "$deps/topos"
      fi
      patches+=(
        "topos-bible = { path = \"$deps/topos/crates/topos-lib\" }"
        "topos-bible-markup = { path = \"$deps/topos/crates/topos-markup\" }"
        "topos-bible-index = { path = \"$deps/topos/crates/topos-index\" }"
        "topos-bible-formats = { path = \"$deps/topos/crates/topos-formats\" }"
      )
      ;;
    sword-modules)
      version=$(version_of sword-modules)
      if published sword-modules "$version"; then
        echo "sword-modules $version is on crates.io"
        continue
      fi
      if [[ -z "${SWORD_MODULES_REPO:-}" ]]; then
        echo "::error::sword-modules $version isn't on crates.io and has no public repository: publish it, or set the SWORD_MODULES_REPO variable to a git URL"
        exit 1
      fi
      echo "sword-modules $version isn't on crates.io: building against $SWORD_MODULES_REPO"
      if [[ ! -d "$deps/sword-modules" ]]; then
        clone "$SWORD_MODULES_REPO" "${SWORD_MODULES_REF:-v$version}" "$deps/sword-modules"
      fi
      patches+=("sword-modules = { path = \"$deps/sword-modules\" }")
      ;;
    *)
      echo "unknown dependency $name (topos, sword-modules)" >&2
      exit 1
      ;;
  esac
done

if [[ ${#patches[@]} -gt 0 ]]; then
  {
    echo "[patch.crates-io]"
    printf '%s\n' "${patches[@]}"
  } >> "$config"
  echo "wrote $config:"
  cat "$config"
fi
