#!/usr/bin/env bash
# The repository setting ADR-0211 decision 2 decides, for the author to apply: a ruleset letting only the repository's
# administrators create, move or delete a `v*` tag, so no workflow or bot can move a release. Dependabot is
# `.github/dependabot.yml`, effective once pushed; requiring `check-all` on `main` waits until pushes go through pull
# requests. Run again, it changes nothing.
#
# Usage: scripts/setup/github-settings.sh [owner/repo]    (default rxdu/object_flow; `gh` logged in as an administrator)
set -euo pipefail
repo="${1:-rxdu/object_flow}"
name="release tags"
if gh api "repos/$repo/rulesets" --jq '.[].name' | grep -qxF "$name"; then
    echo "$repo already holds the ruleset '$name'"
    exit 0
fi
# the repository role 5 is the administrator's, who bypasses the rules; everyone else is held by them
gh api --method POST "repos/$repo/rulesets" --input - <<JSON
{
  "name": "$name",
  "target": "tag",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["refs/tags/v*"], "exclude": [] } },
  "rules": [ { "type": "creation" }, { "type": "update" }, { "type": "deletion" } ],
  "bypass_actors": [ { "actor_id": 5, "actor_type": "RepositoryRole", "bypass_mode": "always" } ]
}
JSON
echo
echo "$repo: only its administrators create, move or delete a v* tag"
