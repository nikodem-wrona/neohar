#!/usr/bin/env python3
"""Post a multi-dimensional code review to a GitHub PR via the gh CLI.

Reads a findings JSON file produced by the `pr-review` skill and posts:

  1. A single PR review with INLINE comments anchored to changed lines, plus an
     overall summary `body`. The review event is REQUEST_CHANGES when any
     Critical finding exists, otherwise COMMENT. It NEVER approves.

  2. A separate GENERAL PR comment (`gh pr comment`) for cross-cutting findings
     that aren't tied to one line, plus any inline finding whose line is not part
     of the PR diff (those are auto-demoted so GitHub can't 422 the whole review).

Why the pre-validation matters:
  - GitHub rejects the ENTIRE review (HTTP 422) if a single inline comment points
    at a line that isn't part of the diff hunk. We parse the diff first and only
    keep comments on commentable lines.
  - GitHub rejects REQUEST_CHANGES / APPROVE on your OWN pull request. If that
    happens we transparently fall back to event=COMMENT while keeping the verdict
    visible in the summary body, so the signal is never lost.

Stdlib only. Requires an authenticated `gh` (gh auth status).

Findings JSON schema:
{
  "summary": "markdown overall summary (becomes the review body)",
  "event": "REQUEST_CHANGES" | "COMMENT",   // optional; auto-derived from severities if omitted
  "inline": [
    {"path": "apps/x.py", "line": 42, "side": "RIGHT",
     "severity": "critical", "title": "SQL injection", "body": "markdown..."},
    {"path": "apps/y.ts", "start_line": 10, "line": 14, "side": "RIGHT",
     "severity": "warning", "title": "N+1 query", "body": "markdown..."}
  ],
  "general": [
    {"severity": "warning", "title": "Migration not generated", "body": "markdown..."}
  ]
}

Usage:
  python3 .claude/skills/pr-review/scripts/post_review.py --pr 852 --findings findings.json
  python3 .claude/skills/pr-review/scripts/post_review.py --pr 852 --findings findings.json --dry-run
"""

import argparse
import json
import re
import subprocess
import sys

SEVERITY_BADGE = {
    "critical": "🔴 **Critical**",
    "warning": "🟡 **Warning**",
    "info": "🔵 **Info**",
}
HUNK_RE = re.compile(r"^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@")


def run(args, stdin=None, check=True):
    """Run a subprocess and return (returncode, stdout, stderr)."""
    proc = subprocess.run(
        args, input=stdin, capture_output=True, text=True
    )
    if check and proc.returncode != 0:
        sys.stderr.write(proc.stderr)
        raise SystemExit(f"command failed ({proc.returncode}): {' '.join(args)}")
    return proc.returncode, proc.stdout, proc.stderr


def get_pr_meta(pr):
    _, out, _ = run(
        ["gh", "pr", "view", pr, "--json",
         "number,headRefOid,headRefName,baseRefName,url,isCrossRepository"]
    )
    return json.loads(out)


def get_diff(pr):
    _, out, _ = run(["gh", "pr", "diff", pr])
    return out


def parse_commentable(diff_text):
    """Map each file path to the set of line numbers commentable per side.

    RIGHT side = new file lines (added + context). LEFT side = old file lines
    (removed + context). Only lines inside a hunk are commentable.
    """
    files = {}
    path = None
    old_ln = new_ln = 0
    in_hunk = False

    for line in diff_text.splitlines():
        if line.startswith("+++ "):
            p = line[4:].strip()
            if p == "/dev/null":
                path = None
            else:
                path = p[2:] if p.startswith("b/") else p
                files.setdefault(path, {"RIGHT": set(), "LEFT": set()})
            in_hunk = False
            continue
        if line.startswith("--- "):
            in_hunk = False
            continue
        m = HUNK_RE.match(line)
        if m:
            old_ln, new_ln = int(m.group(1)), int(m.group(2))
            in_hunk = True
            continue
        if not in_hunk or path is None:
            continue
        if line.startswith("\\"):  # "\ No newline at end of file"
            continue
        if line.startswith("+"):
            files[path]["RIGHT"].add(new_ln)
            new_ln += 1
        elif line.startswith("-"):
            files[path]["LEFT"].add(old_ln)
            old_ln += 1
        else:  # context line (leading space) or blank
            files[path]["RIGHT"].add(new_ln)
            files[path]["LEFT"].add(old_ln)
            new_ln += 1
            old_ln += 1
    return files


def is_commentable(f, files):
    side = (f.get("side") or "RIGHT").upper()
    path = f.get("path")
    if path not in files or "line" not in f:
        return False
    lines = files[path].get(side, set())
    if f["line"] not in lines:
        return False
    if "start_line" in f and f["start_line"] not in lines:
        return False
    return True


def comment_body(f):
    badge = SEVERITY_BADGE.get((f.get("severity") or "info").lower(), "")
    title = f.get("title", "").strip()
    head = " — ".join(x for x in [badge, f"**{title}**" if title else ""] if x)
    parts = [p for p in [head, f.get("body", "").strip()] if p]
    return "\n\n".join(parts)


def build_review_comments(valid):
    out = []
    for f in valid:
        c = {
            "path": f["path"],
            "line": f["line"],
            "side": (f.get("side") or "RIGHT").upper(),
            "body": comment_body(f),
        }
        if "start_line" in f:
            c["start_line"] = f["start_line"]
            c["start_side"] = (f.get("start_side") or c["side"]).upper()
        out.append(c)
    return out


def derive_event(findings):
    if findings.get("event"):
        return findings["event"].upper()
    severities = [
        (f.get("severity") or "").lower()
        for f in findings.get("inline", []) + findings.get("general", [])
    ]
    return "REQUEST_CHANGES" if "critical" in severities else "COMMENT"


def verdict_line(event, counts):
    summary = ", ".join(f"{n} {s}" for s, n in counts.items() if n) or "no issues"
    if event == "REQUEST_CHANGES":
        return f"## ⛔ Changes requested — {summary}"
    return f"## 💬 Review — {summary}"


def render_general(general, demoted):
    blocks = []
    if general:
        lines = ["### General notes (not tied to a line)\n"]
        for f in general:
            lines.append(f"- {comment_body(f)}".replace("\n\n", " — "))
        blocks.append("\n".join(lines))
    if demoted:
        lines = ["### Findings on unchanged lines (could not inline)\n"]
        for f in demoted:
            loc = f"`{f.get('path','?')}:{f.get('line','?')}`"
            lines.append(f"- {loc} {comment_body(f)}".replace("\n\n", " — "))
        blocks.append("\n".join(lines))
    return "\n\n".join(blocks)


def post_review(pr, payload, dry_run):
    endpoint = f"repos/{{owner}}/{{repo}}/pulls/{pr}/reviews"
    body = json.dumps(payload)
    if dry_run:
        print("── DRY RUN: would POST review ──")
        print(f"  endpoint: {endpoint}")
        print(f"  event:    {payload.get('event')}")
        print(f"  inline:   {len(payload.get('comments', []))} comment(s)")
        print("  payload:")
        print(json.dumps(payload, indent=2))
        return
    code, out, err = run(
        ["gh", "api", "-X", "POST", endpoint, "--input", "-"],
        stdin=body, check=False,
    )
    if code != 0 and "own pull request" in (err + out).lower():
        sys.stderr.write("note: cannot REQUEST_CHANGES on own PR; retrying as COMMENT\n")
        payload["event"] = "COMMENT"
        code, out, err = run(
            ["gh", "api", "-X", "POST", endpoint, "--input", "-"],
            stdin=json.dumps(payload), check=False,
        )
    if code != 0 and payload.get("comments"):
        sys.stderr.write("note: review rejected with comments; retrying body-only\n")
        sys.stderr.write(err)
        payload["comments"] = []
        code, out, err = run(
            ["gh", "api", "-X", "POST", endpoint, "--input", "-"],
            stdin=json.dumps(payload), check=False,
        )
    if code != 0:
        sys.stderr.write(err)
        raise SystemExit("failed to post review")
    print("✓ posted review")


def post_general_comment(pr, body, dry_run):
    if not body.strip():
        return
    if dry_run:
        print("── DRY RUN: would post general comment ──")
        print(body)
        return
    run(["gh", "pr", "comment", pr, "--body-file", "-"], stdin=body)
    print("✓ posted general comment")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--pr", required=True, help="PR number")
    ap.add_argument("--findings", required=True, help="path to findings JSON")
    ap.add_argument("--dry-run", action="store_true",
                    help="print what would be posted, post nothing")
    args = ap.parse_args()

    with open(args.findings) as fh:
        findings = json.load(fh)

    meta = get_pr_meta(args.pr)
    files = parse_commentable(get_diff(args.pr))

    inline = findings.get("inline", [])
    valid = [f for f in inline if is_commentable(f, files)]
    demoted = [f for f in inline if not is_commentable(f, files)]

    counts = {"Critical": 0, "Warning": 0, "Info": 0}
    for f in inline + findings.get("general", []):
        key = (f.get("severity") or "info").capitalize()
        if key in counts:
            counts[key] += 1

    event = derive_event(findings)
    summary = findings.get("summary", "").strip()
    body = f"{verdict_line(event, counts)}\n\n{summary}".strip()

    payload = {
        "commit_id": meta["headRefOid"],
        "body": body,
        "event": event,
        "comments": build_review_comments(valid),
    }

    print(f"PR #{meta['number']} ({meta['headRefName']} → {meta['baseRefName']})")
    print(f"  {len(valid)} inline / {len(demoted)} demoted / "
          f"{len(findings.get('general', []))} general · event={event}")

    post_review(args.pr, payload, args.dry_run)
    post_general_comment(
        args.pr, render_general(findings.get("general", []), demoted), args.dry_run
    )
    if not args.dry_run:
        print(meta["url"])


if __name__ == "__main__":
    main()
