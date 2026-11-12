#!/usr/bin/env bash
# Judge a community submission read by read_form.py.
#
# Usage: judge.sh <unilux binary> <submission dir> <issue number> <repo root>
# Writes <submission dir>/comment.md, and on success the recipe and its
# reports under <repo root>/community/recipes/<id>/. Prints id=, custom=
# and ok= lines (for GITHUB_OUTPUT).
set -euo pipefail

unilux="$1" sub="$2" issue="$3" root="$4"
comment="$sub/comment.md"

out() { printf '%s\n' "$@"; }

if ! "$unilux" --json recipe check "$sub/recipe.toml" > "$sub/check.json"; then
    { out "The recipe does not check:" "" '```'; jq -r '.error.message // .' "$sub/check.json"; out '```'; } > "$comment"
    out "ok=false"
    exit 0
fi
id=$(jq -r '.data.id' "$sub/check.json")
custom=$(jq -r '.data.custom_code' "$sub/check.json")
dir="$root/community/recipes/$id"
"$unilux" recipe check --print "$sub/recipe.toml" > "$sub/canonical.toml"
if [ -f "$dir/recipe.toml" ] && ! cmp -s "$dir/recipe.toml" "$sub/canonical.toml"; then
    out "A different recipe named \`$id\` already exists. Give yours another id, or submit reports made with the published one." > "$comment"
    out "ok=false"
    exit 0
fi
if ! jq -e 'type == "array" or type == "object"' "$sub/reports.json" > /dev/null 2>&1; then
    out "The session reports are not JSON. Paste what \`unilux recipe share\` printed." > "$comment"
    out "ok=false"
    exit 0
fi
mkdir -p "$dir/reports"
cp "$sub/canonical.toml" "$dir/recipe.toml"
n=$(jq 'if type == "array" then length else 1 end' "$sub/reports.json")
for i in $(seq 0 $((n - 1))); do
    jq "if type == \"array\" then .[$i] else . end" "$sub/reports.json" > "$dir/reports/issue$issue-$i.json"
done
"$unilux" --json community verify "$dir" > "$sub/verdict.json"
"$unilux" community verify "$dir" > "$sub/verdict.txt"
{
    out "Recipe \`$id\`: $(jq -r '.data.verdict.status' "$sub/verdict.json")" "" '```'
    cat "$sub/verdict.txt"
    out '```'
    if [ "$custom" = "true" ]; then
        out "" "This recipe brings its own Wine patches: a maintainer reads them before merging."
    fi
} > "$comment"
valid=$(jq -r '.data.verdict.reports' "$sub/verdict.json")
out "id=$id" "custom=$custom"
if [ "$valid" -gt 0 ]; then
    out "ok=true"
else
    # Nothing worth a pull request: drop what was written.
    git -C "$root" ls-files --error-unmatch "$dir/recipe.toml" > /dev/null 2>&1 || rm -rf "$dir"
    out "ok=false"
fi
