#!/usr/bin/env python3
"""Read-only upstream intake snapshot. Requires GitHub CLI authentication."""
import argparse
import json
import os
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UPSTREAM = "crmne/zapfast"


def run(*arguments):
    options = {"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {}
    return subprocess.check_output(arguments, cwd=ROOT, text=True, encoding="utf-8", **options)


def snapshot():
    pulls = json.loads(run("gh", "pr", "list", "--repo", UPSTREAM, "--state", "open", "--limit", "500", "--json", "number,title,url,author,headRefOid,updatedAt,isDraft"))
    issues = json.loads(run("gh", "issue", "list", "--repo", UPSTREAM, "--state", "open", "--limit", "500", "--json", "number,title,url,author,updatedAt,labels"))
    ledger = (ROOT / "docs/upstream-intake.md").read_text(encoding="utf-8")
    tracked = {}
    for line in ledger.splitlines():
        match = re.match(r"\| \[#(\d+)\].*?\| `([a-f0-9]+)` \| (.*?) \|$", line)
        if match:
            tracked[int(match[1])] = (match[2], match[3])
    for pull in pulls:
        previous = tracked.get(pull["number"])
        pull["intake"] = "unreviewed" if previous is None else "head changed" if not pull["headRefOid"].startswith(previous[0]) else previous[1]
    return {"upstream": UPSTREAM, "forkCommit": run("git", "rev-parse", "HEAD").strip(), "pullRequests": pulls, "issues": issues}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="Where to save the metadata snapshot")
    options = parser.parse_args()
    data = snapshot()
    options.output.parent.mkdir(parents=True, exist_ok=True)
    options.output.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Saved {len(data['pullRequests'])} open PRs and {len(data['issues'])} open issues to {options.output}")
    for pull in data["pullRequests"]:
        if pull["intake"] in {"unreviewed", "head changed"}:
            print(f"#{pull['number']}: {pull['intake']}: {pull['title']}")


if __name__ == "__main__":
    main()
