# OctoBuddy's entry in `native-apps.json`

English | [简体中文](octobuddy.zh-CN.md)

[OctoBuddy](https://github.com/tyreseluo/OctoBuddy) is the coding app: an outer loop plans the work into slices, inner loops build them in parallel, the host checks and reviews what lands. Its entry asks for more than any other module, so this page says, for each grant, what it is for, what bounds it and what goes if it is taken away. What OctoBuddy keeps and sends is in its own [PRIVACY.md](https://github.com/tyreseluo/OctoBuddy/blob/main/PRIVACY.md).

## Where it runs

- **A module on every target** (`hosting`), linked into the desktop shell by default and left out of the phone shell (`shells`).
- **In the shell's process.** The `sandbox` block is applied only to a process launch (`sandbox::Policy::for_app`, ADR 0004 §3), so today no OS sandbox narrows OctoBuddy: it can do what the shell can, as Rinx, App Hub and AppCard can. The block says what it does, and is the policy it gets if its `hosting` ever moves to `process`.

## The grants

| Grant | What it is for | What bounds it | Without it |
| --- | --- | --- | --- |
| `sandbox.network: "any"` | The model providers the person enabled (through OctoBuddy's loopback proxy, which adds the key), the agents' downloads (npm's registry, octos's GitHub release, each checked against a pinned digest), the app templates repository. | Only the hosts the person's providers and OctoBuddy's pins name; no telemetry (PRIVACY.md). | No model answers; no agent installs. |
| `sandbox.processes: true` | The loops are agent programs OctoBuddy starts: `claude -p`, `codex app-server`, `pi --mode rpc`, its own `octos serve`; and `git` and the project's own checks, which the host runs before it accepts a slice. | Each child gets a placeholder key, never the provider's; the agents run in the project's folder (or its git worktree), which the person picked. | No outer or inner loop: OctoBuddy would be a chat only. |
| `storage.agent_workspace: "account"` | Its app agent (`octos.*` services, the system agent's peer) works in OctoBuddy's account folder, as every app agent with files does (ADR 0004 §11). | That folder only; a request context sees `contexts/<id>/` only. `storage.accounts: false`: one folder for the device. | The app agent reads no files. |
| `agent.octos`: `session.open`, `session.history`, `turn.start`, `turn.interrupt` | A chat on OctoSense's own agent, and the peer the system agent talks to. | The same four services as Rinx. | No chat on OctoSense's agent. |
| `agent.generic_tools` (12) | The octos kernel tools its agent gets in a chat: files (`read_file`, `write_file`, `edit_file`, `glob`, `grep`, `list_dir`), the web (`web_search`, `web_fetch`), `view_image`, `ask_user_question`, `memory_search`, `save_memory`. | Inside its account folder; never octos's shell (`tools/native_apps.py` refuses it for any app). | The chat agent can only talk. |
| `agent.grants`: `terminal.run` | The one way its chat agent can run a command: in the Terminal, where the person sees it. | `terminal.run` is `destructive` and host-confirmed; no standing rule answers it (`auto_approvable: false`): the person confirms every command, live. It is offered only where the Terminal runs as a sandboxed process (ADR 0004 §10, §12). The loops do not use it. | The chat agent cannot run a command; the loops are unchanged. |
| `agent.tools`: `octobuddy.status` (read), `octobuddy.send`, `octobuddy.report`, `octobuddy.request` (act) | What the system agent may ask of OctoBuddy: what runs now, a message to a session's outer loop, a problem with an app it published, a new app to build. | `status` is shareable and read-only. The three `act` tools are host-confirmed: the person confirms each call. `request` only queues: OctoBuddy builds the app when the person presses Build. | The system agent cannot see or reach OctoBuddy. |

## Why not narrower

- **`network: "any"` rather than a list.** The providers are the person's: OctoSense's AI providers app lets them add any endpoint, so the hosts are not known when the entry is reviewed.
- **Inner loops on OctoBuddy's own `octos serve`, not the shell's kernel.** An app's peer cannot yet work in a folder the person picks, share it between loops, run commands or be steered. OctoBuddy's [`docs/upcr-draft-coding-peers.md`](https://github.com/tyreseluo/OctoBuddy/blob/main/docs/upcr-draft-coding-peers.md) drafts what the kernel would need; with it, `processes` could be narrowed to the checks.
- **`terminal.run`** could be dropped without changing the loops; it is kept so a chat on OctoSense's agent can run a command the person sees and confirms.

## Changing it

The entry pins a revision of OctoBuddy (`source.rev`). Moving it is a change to `native-apps.json` and `python3 tools/native_apps.py`; a change to a grant is reviewed here, with this page.
