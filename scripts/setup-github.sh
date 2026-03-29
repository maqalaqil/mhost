#!/bin/bash
# Sets up GitHub repo metadata, labels, and settings.
# Run once after creating the repo: ./scripts/setup-github.sh

set -e

REPO="maheralaqil/mhost"

echo "Setting up GitHub repo: $REPO"
echo "────────────────────────────────"

# ─── Repo description + topics ────────────────────────────────
echo "Setting description and topics..."
gh repo edit "$REPO" \
  --description "AI-powered process manager written in Rust. Single binary, zero dependencies. Health probes, Telegram alerts, Prometheus metrics, reverse proxy, TUI dashboard, cloud fleet management." \
  --homepage "https://mhost.dev" \
  --enable-issues \
  --enable-wiki=false \
  --enable-discussions \
  --enable-projects=false

# ─── Topics ───────────────────────────────────────────────────
echo "Setting topics..."
gh api -X PUT "repos/$REPO/topics" \
  -f '{"names":["process-manager","rust","devops","monitoring","pm2-alternative","telegram-bot","prometheus","ai","cloud","tui","reverse-proxy","health-check","notifications","deploy","cli"]}' \
  --silent 2>/dev/null || \
gh api -X PUT "repos/$REPO/topics" \
  --input - <<EOF
{"names":["process-manager","rust","devops","monitoring","pm2-alternative","telegram-bot","prometheus","ai","cloud","tui","reverse-proxy","health-check","notifications","deploy","cli"]}
EOF

# ─── Labels ───────────────────────────────────────────────────
echo "Creating labels..."

create_label() {
  gh label create "$1" -c "$2" -d "$3" -R "$REPO" 2>/dev/null || \
  gh label edit "$1" -c "$2" -d "$3" -R "$REPO" 2>/dev/null || true
}

# Type
create_label "bug" "d73a4a" "Something isn't working"
create_label "enhancement" "a2eeef" "New feature or improvement"
create_label "question" "d876e3" "Help or clarification needed"
create_label "documentation" "0075ca" "Documentation improvements"
create_label "performance" "fbca04" "Performance improvement"
create_label "security" "e11d48" "Security related"
create_label "breaking" "b60205" "Breaking change"

# Priority
create_label "priority: critical" "b60205" "Must fix immediately"
create_label "priority: high" "d93f0b" "Fix in next release"
create_label "priority: medium" "fbca04" "Fix when possible"
create_label "priority: low" "0e8a16" "Nice to have"

# Status
create_label "triage" "ededed" "Needs triage"
create_label "confirmed" "0e8a16" "Bug confirmed"
create_label "wont-fix" "ffffff" "Will not be fixed"
create_label "duplicate" "cfd3d7" "Duplicate issue"
create_label "good first issue" "7057ff" "Good for newcomers"
create_label "help wanted" "008672" "Extra attention needed"
create_label "stale" "ededed" "Inactive issue"

# Area
for area in core health notify logs metrics proxy deploy tui ai cloud bot cli config daemon; do
  create_label "area: $area" "1d76db" "$area subsystem"
done

# CI
create_label "dependencies" "0366d6" "Dependency update"
create_label "rust" "dea584" "Rust related"
create_label "ci" "ededed" "CI/CD pipeline"
create_label "npm" "cb3837" "npm package"

# ─── Branch protection ────────────────────────────────────────
echo "Setting branch protection..."
gh api -X PUT "repos/$REPO/branches/main/protection" \
  --input - <<EOF 2>/dev/null || echo "  (skipped — may need admin access)"
{
  "required_status_checks": {
    "strict": true,
    "contexts": ["Test (ubuntu-latest)", "Test (macos-latest)", "Test (windows-latest)", "Lint"]
  },
  "enforce_admins": false,
  "required_pull_request_reviews": null,
  "restrictions": null
}
EOF

# ─── Enable Pages ─────────────────────────────────────────────
echo "Enabling GitHub Pages..."
gh api -X POST "repos/$REPO/pages" \
  --input - <<EOF 2>/dev/null || echo "  (Pages may already be enabled)"
{
  "build_type": "workflow"
}
EOF

echo ""
echo "Done! Your repo is set up."
echo ""
echo "Next steps:"
echo "  1. Add secrets: NPM_TOKEN, CARGO_TOKEN, HOMEBREW_TAP_TOKEN"
echo "  2. Create homebrew-tap repo: gh repo create maheralaqil/homebrew-tap --public"
echo "  3. Push to main to trigger first release"
