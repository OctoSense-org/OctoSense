#!/usr/bin/env python3
"""Merge a pull request on a local CI pass (Python 3.9+; docs/local-ci.md).

  tools/ci-local-merge.sh <PR number> [--dry-run] [--fixes-main]

Refuses unless target/ci-local/last.json (from tools/ci-local.sh) shows:

- a pass on the PR's exact head commit, from a clean tree;
- that head already contains the current origin/main (otherwise: merge or
  rebase main into the branch, push, and run tools/ci-local.sh again);
- every workflow GitHub would run for the PR's files (its pull_request
  paths), with no FAIL, no step left NOT RUN and no unexpected SKIP in them;
  a workflow GitHub would not run for the PR does not block it;
- steps that ran on the Linux host (--linux-host; `"host": "linux"`) count
  like local ones, but only if the host ran that same head commit: a remote
  result bound to another commit is stale. The Linux-only checks
  (ci_local.LINUX_HOST_JOBS) count for the PRs that trigger the workflows
  they cover.

It also refuses while the last completed push run of a workflow on main
failed: a red main is fixed first (--fixes-main for the PR that fixes it).
Then it posts the summary table as a PR comment ("Local CI passed on <sha>")
and runs `gh pr merge <n> --admin --merge`. The merge commit is an ordinary
push to main: GitHub CI runs on it (the workflows' `<workflow>-main`
concurrency group cancels older main runs, so the queue does not pile up).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("ci_local", ROOT / "tools/ci_local.py")
ci_local = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ci_local)


class Refused(Exception):
    pass


def run(argv, check=True):
    result = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True)
    if check and result.returncode != 0:
        raise Refused(f"`{' '.join(argv)}` failed: {result.stderr.strip() or result.stdout.strip()}")
    return result


def gh_json(argv):
    return json.loads(run(["gh", *argv]).stdout)


def evidence_problems(last, head, changed_files):
    """Why `last` (target/ci-local/last.json) is not evidence for merging
    `head` touching `changed_files`; [] when it is."""
    problems = []
    if last.get("sha") != head:
        problems.append(f"stale: the local run was on {str(last.get('sha'))[:12]}, the PR head is {head[:12]}")
    if last.get("dirty"):
        problems.append("the local run had uncommitted changes to tracked files")
    required = ci_local.triggered_workflows(changed_files)
    missing = [w for w in required if w not in last.get("workflows", [])]
    if missing:
        problems.append(f"the run did not cover {', '.join(missing)} (GitHub runs them for this PR): "
                        f"run tools/ci-local.sh --only all")
    scope = set(required) | {"ci-local"}
    for job_id, job in ci_local.LINUX_HOST_JOBS.items():
        if set(ci_local.linux_host_job_covers(job)) & set(required):
            scope.add(ci_local.LINUX_HOST)
    stale = [s for s in last.get("steps", []) if s.get("host") == "linux" and s.get("sha") != head]
    if stale:
        ran = sorted({str(s.get("sha"))[:12] for s in stale})
        problems.append(f"stale: {len(stale)} step(s) on the Linux host ran on {', '.join(ran)}, not the PR head "
                        f"{head[:12]} (first: {stale[0]['workflow']} / {stale[0]['job']}: {stale[0]['name']})")
    for step in last.get("steps", []):
        where = f"{step['workflow']} / {step['job']}: {step['name']}"
        if step in stale:
            continue
        if step.get("host") == "linux":
            where += " (Linux host)"
        if step["workflow"] not in scope:
            continue  # GitHub would not run it for this PR either
        if step["status"] == ci_local.FAIL:
            problems.append(f"failed: {where}")
        elif step["status"] == ci_local.NOT_RUN:
            problems.append(f"not run: {where}")
        elif step["status"] == ci_local.SKIPPED and not step.get("expected_skip"):
            problems.append(f"skipped: {where} ({step.get('reason')})")
    return problems, required


# Native Windows acceptance cannot be replaced by a macOS/Linux local pass.
REMOTE_REQUIRED = ("embedded-browser.yml",)


def remote_evidence_problems(head, changed_files):
    problems = []
    required = []
    for workflow in REMOTE_REQUIRED:
        paths = ci_local.pull_request_paths(workflow)
        if paths is not None and not any(ci_local.glob_match(f, p) for f in changed_files for p in paths):
            continue
        required.append(workflow)
        runs = gh_json(["run", "list", "--workflow", workflow, "--commit", head,
                        "--event", "pull_request", "--limit", "1", "--json", "headSha,status,conclusion"])
        if not runs or runs[0].get("headSha") != head or runs[0].get("status") != "completed" or runs[0].get("conclusion") != "success":
            problems.append(f"{workflow}: native GitHub acceptance has not passed on {head[:12]}")
    return problems, required


RED = ("failure", "timed_out", "startup_failure")


def red_main(repo_workflows):
    red = []
    for workflow in repo_workflows:
        result = run(["gh", "run", "list", "--branch", "main", "--event", "push", "--workflow", workflow,
                      "--status", "completed", "--limit", "20", "--json", "conclusion,headSha,url"], check=False)
        if result.returncode != 0:
            continue
        # The latest run that reached a verdict: cancelled runs were superseded.
        runs = [r for r in json.loads(result.stdout or "[]") if r.get("conclusion") not in ("cancelled", "skipped")]
        if runs and runs[0].get("conclusion") in RED:
            red.append(f"{workflow}: {runs[0].get('conclusion')} on {runs[0]['headSha'][:12]} ({runs[0]['url']})")
    return red


def comment_body(last, required):
    head = last["sha"]
    linux = last.get("linux_host") or {}
    where = f"{last['host']['system']} {last['host']['machine']}"
    flag = ""
    if linux:
        flag = " --linux-host"
        where += f"; the ubuntu jobs on the Linux build host, {linux.get('system')} {linux.get('machine')}"
    lines = [f"Local CI passed on {head} (`tools/ci-local.sh --only {last['only']}{flag}`, "
             f"{ci_local.fmt_seconds(last['seconds'])}, {where}).",
             "",
             f"GitHub would run for this PR: {', '.join(required) or 'none'}. Merged on this local "
             f"pass (macOS runner queue); GitHub CI runs on the merge commit on main.",
             "",
             ci_local.format_table(last, markdown=True)]
    outside = [s for s in last.get("steps", []) if s["workflow"] not in set(required) | {"ci-local"}
               and s["status"] in (ci_local.FAIL, ci_local.NOT_RUN)]
    if outside:
        lines[4:4] = [f"Outside those workflows {len(outside)} step(s) did not pass (in the table); "
                      f"GitHub would not run them for this PR.", ""]
    return "\n".join(lines) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("pr", type=int, help="The pull request number")
    parser.add_argument("--dry-run", action="store_true", help="Check and print the comment; merge nothing")
    parser.add_argument("--fixes-main", action="store_true",
                        help="Merge although main's last GitHub run failed (this PR fixes it)")
    args = parser.parse_args(argv)
    try:
        info = gh_json(["pr", "view", str(args.pr), "--json",
                        "number,state,isDraft,baseRefName,headRefName,headRefOid,headRepositoryOwner,url"])
        if info["state"] != "OPEN":
            raise Refused(f"PR #{args.pr} is {info['state']}")
        if info["isDraft"]:
            raise Refused(f"PR #{args.pr} is a draft")
        if info["baseRefName"] != "main":
            raise Refused(f"PR #{args.pr} targets {info['baseRefName']}, not main")
        head = info["headRefOid"]
        last_path = ROOT / "target/ci-local/last.json"
        if not last_path.is_file():
            raise Refused("no target/ci-local/last.json: run tools/ci-local.sh on the PR head first")
        last = json.loads(last_path.read_text())

        run(["git", "fetch", "--quiet", "origin", "main"])
        if run(["git", "cat-file", "-e", f"{head}^{{commit}}"], check=False).returncode != 0:
            run(["git", "fetch", "--quiet", "origin", f"pull/{args.pr}/head"])
        main_sha = run(["git", "rev-parse", "origin/main"]).stdout.strip()
        if run(["git", "merge-base", "--is-ancestor", main_sha, head], check=False).returncode != 0:
            raise Refused(f"the head {head[:12]} does not contain origin/main {main_sha[:12]}: rebase or merge "
                          f"main into the branch, push, run tools/ci-local.sh again, then retry")
        changed = run(["gh", "pr", "diff", str(args.pr), "--name-only"]).stdout.split()
        problems, required = evidence_problems(last, head, changed)
        if problems:
            raise Refused("the local run is not evidence for this merge:\n  - " + "\n  - ".join(problems))
        remote_problems, remote_required = remote_evidence_problems(head, changed)
        if remote_problems:
            raise Refused("native acceptance cannot be replaced by local CI:\n  - " + "\n  - ".join(remote_problems))
        red = red_main(ci_local.GROUPS["all"] + list(REMOTE_REQUIRED))
        if red and not args.fixes_main:
            raise Refused("main is red on GitHub; fix it first (or pass --fixes-main for the fix):\n  - "
                          + "\n  - ".join(red))

        body = comment_body(last, required)
        if remote_required:
            body += "\nNative GitHub acceptance passed on the same head: " + ", ".join(remote_required) + ".\n"
        owner = (info.get("headRepositoryOwner") or {}).get("login", "")
        subject = f"Merge pull request #{args.pr} from {owner}/{info['headRefName']}"
        merge_body = f"Local CI (tools/ci-local.sh --only {last['only']}) passed on {head}."
        if args.dry_run:
            print(f"ci-local-merge: would comment on {info['url']}:\n\n{body}")
            print(f"ci-local-merge: would run gh pr merge {args.pr} --admin --merge --subject '{subject}'")
            return 0
        with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False) as handle:
            handle.write(body)
        run(["gh", "pr", "comment", str(args.pr), "--body-file", handle.name])
        Path(handle.name).unlink()
        run(["gh", "pr", "merge", str(args.pr), "--admin", "--merge", "--match-head-commit", head,
             "--subject", subject, "--body", merge_body])
        merged = gh_json(["pr", "view", str(args.pr), "--json", "mergeCommit,state"])
        commit = (merged.get("mergeCommit") or {}).get("oid", "?")
        print(f"ci-local-merge: merged {info['url']} as {commit} ({subject})")
        return 0
    except Refused as refusal:
        print(f"ci-local-merge: refused: {refusal}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
