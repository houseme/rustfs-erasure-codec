#!/usr/bin/env bash
set -euo pipefail

package=rustfs-erasure-codec
version=9.1.0
tag=9.1.0
: "${GITHUB_SHA:?a tested source commit is required}"
: "${GITHUB_REPOSITORY:?the source repository is required}"
: "${CARGO_REGISTRY_TOKEN:?the crates.io publishing token is required}"
: "${GH_TOKEN:?the repository publishing token is required}"
[[ "${GITHUB_REF:-}" == refs/heads/main ]] || exit 1
[[ "$GITHUB_SHA" =~ ^[0-9a-f]{40}$ ]] || exit 1
[[ "$(git rev-parse HEAD)" == "$GITHUB_SHA" ]] || exit 1
[[ -z "$(git status --porcelain --untracked-files=no)" ]] || exit 1
cargo xtask check-release --tag "$tag"

verify_tag() {
    git fetch --quiet --no-tags origin "refs/tags/$tag"
    [[ "$(git rev-parse 'FETCH_HEAD^{commit}')" == "$GITHUB_SHA" ]] || exit 1
}

publication="${CARGO_TARGET_DIR:-target}/publication"
mkdir -p "$publication"
if git ls-remote --exit-code --tags origin "refs/tags/$tag" > "$publication/tag-before.txt"; then
    verify_tag
else
    result=$?
    [[ "$result" -eq 2 ]] || exit 1
fi

cargo publish --dry-run --locked --registry crates-io --package "$package"
# Materialize the archive independently of Cargo's publish staging directory.
cargo package --locked --registry crates-io --package "$package"
archive="${CARGO_TARGET_DIR:-target}/package/$package-$version.crate"
[[ "$(tar -xOf "$archive" "$package-$version/.cargo_vcs_info.json" | jq -er '.git.sha1')" == "$GITHUB_SHA" ]] || exit 1

agent="rustfs-erasure-codec-release (https://github.com/$GITHUB_REPOSITORY)"
endpoint="https://crates.io/api/v1/crates/$package/$version"
registry_status() {
    curl --silent --show-error --location --retry 3 --retry-delay 2 \
        --user-agent "$agent" --output "$publication/registry.json" \
        --write-out '%{http_code}' "$endpoint"
}

verify_registry() {
    local expected actual
    curl --fail --silent --show-error --location --retry 3 --user-agent "$agent" \
        "$endpoint/download" --output "$publication/registry.crate"
    expected=$(jq -er '.version.checksum' "$publication/registry.json")
    actual=$(sha256sum "$publication/registry.crate" | cut -d ' ' -f 1)
    [[ "$actual" == "$expected" ]] || { printf 'Registry archive checksum mismatch\n' >&2; exit 1; }
    [[ "$(sha256sum "$archive" | cut -d ' ' -f 1)" == "$expected" ]] || { printf 'Local package differs from registry\n' >&2; exit 1; }
    [[ "$(tar -xOf "$publication/registry.crate" "$package-$version/.cargo_vcs_info.json" | jq -er '.git.sha1')" == "$GITHUB_SHA" ]] || { printf 'Registry source commit mismatch\n' >&2; exit 1; }
    jq -n --arg version "$version" --arg source "$GITHUB_SHA" --arg checksum "$expected" \
        '{version:$version,source_commit:$source,registry_checksum:$checksum,qualification:"functional-and-compatibility",registry_verified:true}' \
        > "$publication/receipt.json"
}

status=$(registry_status)
case "$status" in
    200) verify_registry ;;
    404)
        cargo publish --locked --registry crates-io --package "$package"
        for attempt in {1..30}; do
            status=$(registry_status)
            if [[ "$status" == 200 ]]; then
                break
            fi
            [[ "$status" == 404 ]] || exit 1
            printf 'Waiting for registry indexing, attempt %s\n' "$attempt"
            sleep 10
        done
        [[ "$status" == 200 ]] || exit 1
        verify_registry
        ;;
    *) printf 'Unexpected registry response %s\n' "$status" >&2; exit 1 ;;
esac

if git ls-remote --exit-code --tags origin "refs/tags/$tag" > "$publication/tag-after.txt"; then
    verify_tag
else
    result=$?
    [[ "$result" -eq 2 ]] || exit 1
    gh api --method POST "repos/$GITHUB_REPOSITORY/git/refs" \
        -f "ref=refs/tags/$tag" -f "sha=$GITHUB_SHA" --silent
    verify_tag
fi

if gh release view "$tag" --repo "$GITHUB_REPOSITORY" --json isDraft > "$publication/release-before.json"; then
    gh release edit "$tag" --repo "$GITHUB_REPOSITORY" --draft=false --prerelease=false --latest
else
    gh release create "$tag" --repo "$GITHUB_REPOSITORY" --verify-tag \
        --title "$version" --notes-file docs/release-9.1.0-notes.md --latest
fi
gh release view "$tag" --repo "$GITHUB_REPOSITORY" --json url,tagName,isDraft,isPrerelease \
    > "$publication/release.json"
if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf '## 9.1.0 publication\n\n- Source %s\n- Functional and compatibility qualification\n- Registry archive verified\n- Tag %s verified\n' \
        "$GITHUB_SHA" "$tag" >> "$GITHUB_STEP_SUMMARY"
fi
