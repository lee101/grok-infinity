<div align="center">

<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://media.x.ai/v1/website/spacexai-symbol-white-transparent-0c31957f.png">
    <source media="(prefers-color-scheme: light)" srcset="https://media.x.ai/v1/website/spacexai-symbol-black-transparent-6435cf42.png">
    <img alt="SpaceXAI logo" src="https://media.x.ai/v1/website/spacexai-symbol-black-transparent-6435cf42.png" width="96">
  </picture>
  <br>
  Grok Infinity (<code>ginf</code>)
</h1>

**Grok Build** is SpaceXAI's terminal-based AI coding agent. It runs as a
full-screen TUI that understands your codebase, edits files, executes shell
commands, searches the web, and manages long-running tasks — interactively,
headlessly for scripting/CI, or embedded in editors via the Agent Client
Protocol (ACP).

[Installing the released binary](#installing-the-released-binary) ·
[Building from source](#building-from-source) ·
[Documentation](#documentation) ·
[Repository layout](#repository-layout) ·
[Development](#development) ·
[Contributing](#contributing) ·
[License](#license)

![Grok Build TUI](https://media.x.ai/v1/website/universe-tui-screenshot-6f7a0837.png)

**Learn more about Grok Build at [x.ai/cli](https://x.ai/cli)**

**Grok Infinity** extends Grok Build with autonomous continuation modes and
multi-provider model configuration. Project links:
[codex-infinity.com](https://codex-infinity.com) · [app.nz](https://app.nz) ·
[GitHub](https://github.com/lee101/grok-infinity)

This repository contains the Rust source for the `ginf` CLI/TUI and its agent
runtime. It is synced periodically from the upstream SpaceXAI monorepo.

</div>

---

## Installing the released binary

The upstream Grok Build binaries are published for macOS, Linux, and Windows:

```sh
curl -fsSL https://x.ai/cli/install.sh | bash   # macOS / Linux / Git Bash
irm https://x.ai/cli/install.ps1 | iex          # Windows PowerShell
grok --version
```

See the [changelog](https://x.ai/build/changelog) for the latest fixes,
features, and improvements in each release.

## Building from source

Requirements:

- **Rust** — the toolchain is pinned by [`rust-toolchain.toml`](rust-toolchain.toml);
  `rustup` installs it automatically on first build.
- **protoc** — proto codegen resolves [`bin/protoc`](bin/protoc) (a
  [dotslash](https://dotslash-cli.com) launcher) or falls back to a `protoc` on
  `PATH` / `$PROTOC`.
- macOS and Linux are supported build hosts; Windows builds are best-effort
  and not currently tested from this tree.

```sh
cargo run -p xai-grok-pager-bin              # build + launch the TUI
cargo build -p xai-grok-pager-bin --release  # release binary: target/release/ginf
cargo check -p xai-grok-pager-bin            # fast validation
```

## Infinity modes

Grok Infinity can keep working after a normal turn finishes:

```sh
ginf --auto-next-steps "finish the API migration and verify it"
ginf --auto-next-idea
ginf --auto-next-goal "/goal improve test reliability"
ginf --always-approve --auto-next-steps --auto-next-idea
```

| Flag | Behavior |
|------|----------|
| `--auto-next-steps` | Implements and verifies the most important natural follow-up work after each successful turn |
| `--auto-next-idea` | Finds and implements a fresh, useful repository improvement after each successful turn |
| `--auto-next-goal` | Creates a new `/goal` when the current goal reaches `complete` |

These modes intentionally have no turn limit. Normal cancellation, permission,
sandbox, queue, and goal controls still apply. Combining steps and idea mode
finishes immediate follow-up work before moving on to a new improvement.

## Models and providers

Grok Infinity retains Grok Build's generic provider configuration. Add entries
to `~/.grok/config.toml` using any OpenAI-compatible Chat Completions or
Responses endpoint, an Anthropic Messages endpoint, a local model server, or a
gateway such as OpenRouter. Credentials can be literal or read from one or more
environment variables.

```toml
[model.openai]
model = "gpt-5.6-sol"
name = "OpenAI"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
api_backend = "responses"
context_window = 1050000
agent_type = "codex"

[model.anthropic]
model = "claude-sonnet-4-5"
name = "Anthropic"
base_url = "https://api.anthropic.com/v1"
env_key = "ANTHROPIC_API_KEY"
api_backend = "messages"
auth_scheme = "x_api_key"
context_window = 200000

[model.openrouter]
model = "openai/gpt-5.6-sol"
name = "OpenRouter"
base_url = "https://openrouter.ai/api/v1"
env_key = "OPENROUTER_API_KEY"
api_backend = "chat_completions"
context_window = 1050000
```

On startup, `ginf` automatically reuses an eligible OpenAI ChatGPT/Codex
subscription—including Max-plan access—from the existing Codex login:

```sh
codex login
```

When the credential is usable, the default is `openai-max`, routed to
`gpt-5.6-sol` over the Responses API. When no usable subscription credential
is available, the default is `grok-4.5`. An explicit `--model`,
`GROK_DEFAULT_MODEL`, or `[models].default` continues to win.

File credentials are read from `$CODEX_HOME/auth.json` or
`~/.codex/auth.json` on every model request. If the access token is near
expiry, `ginf` refreshes it during startup and a lightweight background check
keeps long-running Infinity sessions authenticated. Refreshes are serialized
between `ginf` processes, and a compare-before-write check prevents overwriting
credentials rotated concurrently by Codex, Codex Infinity, or another
compatible client. A rotated refresh token is never reused after another
client wins the race.

For OS-keychain logins and trusted automation, provide
`CODEX_ACCESS_TOKEN` and `CHATGPT_ACCOUNT_ID`. The token remains sensitive and
is never printed or copied into Grok's config.

The Grok Infinity binary and command are named `ginf`, so they can coexist with
the upstream `grok` command. The upstream login flow remains available — see the
[authentication guide](crates/codegen/xai-grok-pager/docs/user-guide/02-authentication.md).

## Documentation

Full online documentation is available at
[docs.x.ai/build/overview](https://docs.x.ai/build/overview).

The user guide ships with the pager crate:
[`crates/codegen/xai-grok-pager/docs/user-guide/`](crates/codegen/xai-grok-pager/docs/user-guide/)
— getting started, keyboard shortcuts, slash commands, configuration, theming,
MCP servers, skills, plugins, hooks, headless mode, sandboxing, and more.

## Repository layout

| Path | Contents |
|------|----------|
| `crates/codegen/xai-grok-pager-bin` | Composition-root package; builds the `ginf` binary |
| `crates/codegen/xai-grok-pager` | The TUI: scrollback, prompt, modals, rendering |
| `crates/codegen/xai-grok-shell` | Agent runtime + leader/stdio/headless entry points |
| `crates/codegen/xai-grok-tools` | Tool implementations (terminal, file edit, search, ...) |
| `crates/codegen/xai-grok-workspace` | Host filesystem, VCS, execution, checkpoints |
| `crates/codegen/...` | The rest of the CLI crate closure (config, MCP, markdown, sandbox, ...) |
| `crates/common/`, `crates/build/`, `prod/mc/` | Small shared leaf crates pulled in by the closure |
| `third_party/` | Vendored upstream source (Mermaid diagram stack) — see below |

> [!IMPORTANT]
> The root `Cargo.toml` (workspace members, dependency versions, lints,
> profiles) is **generated** — treat it as read-only. Prefer editing per-crate
> `Cargo.toml` files.

## Development

```sh
cargo check -p <crate>        # always target specific crates; full-workspace builds are slow
cargo test -p xai-grok-config # per-crate tests
cargo clippy -p <crate>       # lint config: clippy.toml at the repo root
cargo fmt --all               # rustfmt.toml at the repo root
```

## Contributing

> [!NOTE]
> External contributions are not accepted. See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

First-party code in this repository is licensed under the **Apache License,
Version 2.0** — see [`LICENSE`](LICENSE).

Third-party and vendored code remains under its original licenses. See:

- [`THIRD-PARTY-NOTICES`](THIRD-PARTY-NOTICES) — crates.io / git dependencies,
  bundled UI themes, and **in-tree source ports** (including openai/codex and
  sst/opencode tool implementations)
- [`crates/codegen/xai-grok-tools/THIRD_PARTY_NOTICES.md`](crates/codegen/xai-grok-tools/THIRD_PARTY_NOTICES.md)
  — crate-local notice for the codex and opencode ports (license texts +
  Apache §4(b) change notice)
- [`third_party/NOTICE`](third_party/NOTICE) — vendored Mermaid-stack index
