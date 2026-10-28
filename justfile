# Release helpers. Both recipes trigger the "Manual Release" workflow on main,
# which bumps Cargo.toml, tags and publishes the GitHub release.

# Version currently on origin/main (e.g. 0.1.2-beta.2)
[private]
main-version:
    @git fetch -q origin main
    @git show origin/main:Cargo.toml | grep '^version' | head -1 | sed 's/version = "\(.*\)"/\1/'

# Publish a stable release (default: current main version without its -beta.N suffix)
release version="":
    #!/usr/bin/env bash
    set -euo pipefail
    current=$(just main-version)
    version="{{ version }}"
    version="${version:-${current%%-*}}"
    echo "main: $current → stable release v$version"
    read -rp "Continue? [y/N] " ok && [[ "$ok" == [yY] ]] || exit 1
    gh workflow run manual-release.yml --ref main -f version="$version"
    sleep 3
    gh run watch "$(gh run list --workflow=manual-release.yml --limit 1 --json databaseId -q '.[0].databaseId')" --exit-status
    echo "✅ v$version released. Run 'just beta' to restart the beta cycle."

# Start a new beta cycle (default: next patch, e.g. 0.1.2 → 0.1.3-beta.0)
beta version="":
    #!/usr/bin/env bash
    set -euo pipefail
    current=$(just main-version)
    version="{{ version }}"
    if [[ -z "$version" ]]; then
        IFS=. read -r major minor patch <<< "${current%%-*}"
        version="$major.$minor.$((patch + 1))-beta.0"
    fi
    echo "main: $current → v$version"
    read -rp "Continue? [y/N] " ok && [[ "$ok" == [yY] ]] || exit 1
    gh workflow run manual-release.yml --ref main -f version="$version"
    sleep 3
    gh run watch "$(gh run list --workflow=manual-release.yml --limit 1 --json databaseId -q '.[0].databaseId')" --exit-status
    echo "✅ v$version published. Next pushes to main will auto-bump beta.N."
