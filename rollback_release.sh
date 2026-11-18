#!/usr/bin/env bash
set -euo pipefail
tag=${1:?Pass the previously verified healthy version tag}
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]
repo=micsnq2yoma-lgtm/AgenTeam
eligible=$(gh release view "$tag" --repo "$repo" --json isDraft,isPrerelease,assets --jq '(.isDraft == false) and (.isPrerelease == false) and any(.assets[]; .name == "latest.json")')
[[ "$eligible" == true ]]
gh release edit "$tag" --repo "$repo" --latest
