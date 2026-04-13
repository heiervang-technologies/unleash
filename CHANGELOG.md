# Changelog

## [0.1.5](https://github.com/heiervang-technologies/unleash/compare/v0.1.4...v0.1.5) (2026-04-13)


### Features

* token counting, sandbox improvements, supercompact tuning ([#82](https://github.com/heiervang-technologies/unleash/issues/82)) ([2efc9d4](https://github.com/heiervang-technologies/unleash/commit/2efc9d490709ea35dc6e46639d076cfeb08a308c))

## [0.1.4](https://github.com/heiervang-technologies/unleash/compare/v0.1.3...v0.1.4) (2026-04-12)


### Bug Fixes

* **supercompact:** sync plugin.json threshold + audit fixes ([#80](https://github.com/heiervang-technologies/unleash/issues/80)) ([7cc2ce4](https://github.com/heiervang-technologies/unleash/commit/7cc2ce42d62383959888c80e59109929f4dbeafc))

## [0.1.3](https://github.com/heiervang-technologies/unleash/compare/v0.1.2...v0.1.3) (2026-04-12)


### Bug Fixes

* **supercompact:** correct bytes/token ratio from 4 to 12 ([#78](https://github.com/heiervang-technologies/unleash/issues/78)) ([8f22586](https://github.com/heiervang-technologies/unleash/commit/8f22586accbb305c7383a19bd13d40550f836415))

## [0.1.2](https://github.com/heiervang-technologies/unleash/compare/v0.1.1...v0.1.2) (2026-04-12)


### Features

* add native hub/ucf format mode for sessions ([a70487a](https://github.com/heiervang-technologies/unleash/commit/a70487accd7a4662a3b98c51cacc097b0ab6abb7))
* binary smoke test CI + npm-missing warning with install link ([c5fd1d5](https://github.com/heiervang-technologies/unleash/commit/c5fd1d53cf2a599e718f3c7782a28eb0590ebee1))
* curl-based npm registry + interactive nvm install prompt ([72f4e22](https://github.com/heiervang-technologies/unleash/commit/72f4e22ee639e558259e24f2c7d3d4f2f7d963ab))
* **docker:** sandbox verification tests, local API access, and env file ([#67](https://github.com/heiervang-technologies/unleash/issues/67)) ([b81baed](https://github.com/heiervang-technologies/unleash/commit/b81baed703c0aaa1b149871333832353fb70bcca)), closes [#50](https://github.com/heiervang-technologies/unleash/issues/50)
* implement OpenCode SQLite injection for crossload ([#62](https://github.com/heiervang-technologies/unleash/issues/62)) ([e2a9800](https://github.com/heiervang-technologies/unleash/commit/e2a98006bc98ae10ee2021a315a83e425a1f9fb1))
* JSON output for agents info and agents list subcommands ([f2e951e](https://github.com/heiervang-technologies/unleash/commit/f2e951e4d6159d9946e31095f7b98a0eda7f8695))
* lossless interchange + full polyfill coverage + phantom bug fix ([565114b](https://github.com/heiervang-technologies/unleash/commit/565114b0fe02948af69582fad0b7d76747f96213))
* **polyfill:** add --allowed-tools unified flag (Claude --allowedTools, Gemini --allowed-tools) ([75aded5](https://github.com/heiervang-technologies/unleash/commit/75aded5e35ae29ae2632afa3229c933086021d2b))
* **polyfill:** add --approval-mode unified flag (Claude --permission-mode, Codex -a, Gemini --approval-mode) ([28f43d0](https://github.com/heiervang-technologies/unleash/commit/28f43d031c31c285ea5e241b221c715851a8a4c0))
* **polyfill:** add --name and --add-dir unified flags (Claude --name/--add-dir, Codex --add-dir, Gemini --include-directories) ([5ae16f6](https://github.com/heiervang-technologies/unleash/commit/5ae16f60b91a96d5e0dcc99aeaea96b2fe812063))
* **polyfill:** add --output-format unified flag (Claude --output-format, Gemini -o) ([0c223ef](https://github.com/heiervang-technologies/unleash/commit/0c223ef214271d775dde9c780c20c33082424c26))
* **polyfill:** add --sandbox unified flag with SandboxStrategy enum (Codex --sandbox workspace-write, Gemini --sandbox) ([105995b](https://github.com/heiervang-technologies/unleash/commit/105995b1f4688569a4c1340b788ac1021b45bd9d))
* **polyfill:** add --system-prompt unified flag (Claude --system-prompt) ([730118e](https://github.com/heiervang-technologies/unleash/commit/730118e42eabd22b47b5024a3b2efca92dc788ef))
* **polyfill:** add --verbose unified flag (Claude --verbose, Gemini --debug, OpenCode --print-logs) ([2908687](https://github.com/heiervang-technologies/unleash/commit/290868793a2f0e4f4b58bb3798ee4b7fbbe6b961))
* **polyfill:** add --worktree unified flag (Claude --worktree, Gemini --worktree) ([baf0bfb](https://github.com/heiervang-technologies/unleash/commit/baf0bfbb4a2d72b0c58c6d9c77832d749a4e3af8))
* **polyfill:** wire --auto flag to agent-specific equivalents (Codex --full-auto) ([04c3946](https://github.com/heiervang-technologies/unleash/commit/04c3946d7daf155ff138891b0683ba3714c2631d))
* **supercompact:** include EITF stats table in restart message ([#75](https://github.com/heiervang-technologies/unleash/issues/75)) ([e9a47ec](https://github.com/heiervang-technologies/unleash/commit/e9a47ec29d581eaded83437e95c85f823ee578b1))
* TUI dialog for npm/Node.js install when missing ([e468608](https://github.com/heiervang-technologies/unleash/commit/e46860830efafed94728698d0632022099abfcf8))
* unleash sandbox subcommand ([#70](https://github.com/heiervang-technologies/unleash/issues/70)) ([18366c2](https://github.com/heiervang-technologies/unleash/commit/18366c299f897ac75f1f1c77232ae1e7be24ef33))


### Bug Fixes

* add gh back to docker publish CLI verification ([ec2abeb](https://github.com/heiervang-technologies/unleash/commit/ec2abeb1fe8b7f968d87ea703f38ece507bc9a2b))
* add missing subcommands to RESERVED_NAMES in profile manager ([6f2dfaa](https://github.com/heiervang-technologies/unleash/commit/6f2dfaae3cef487aa9e51b55dd7c65fa58dfaf1b))
* add UserPromptSubmit and SessionEnd to hooks add error message ([98006de](https://github.com/heiervang-technologies/unleash/commit/98006de07aab8c02637cd382273a47843ead1d83))
* agent install, update, and version management bugs ([0e528ba](https://github.com/heiervang-technologies/unleash/commit/0e528baca195c38783625c5511efa1bc9c82d14e))
* avoid spawning claude --version twice in show_current_json ([3ef4bdb](https://github.com/heiervang-technologies/unleash/commit/3ef4bdbd1549caea72932e1435e36e840923780e))
* cap month to 12 in chrono_like_now to prevent invalid filenames ([dcd74b9](https://github.com/heiervang-technologies/unleash/commit/dcd74b90e94d5b578c6ec1371ebe6c7357d511b4))
* cap month to 12 in format_epoch_ms to prevent invalid dates ([a668666](https://github.com/heiervang-technologies/unleash/commit/a6686668eccfc28ed4dbece2a7a2241566dad1f1))
* CLI argument parsing, flag guards, subcommand routing, and recursion safety ([ba029b9](https://github.com/heiervang-technologies/unleash/commit/ba029b94170aa435045c3915a6428724f3bf5089))
* crossload profile-name stripping works for custom profile names ([289b64e](https://github.com/heiervang-technologies/unleash/commit/289b64ebaaf328c434081acddf83ce5f90777b76))
* detect and abort recursive unleash invocation in launcher ([eb75291](https://github.com/heiervang-technologies/unleash/commit/eb7529113369deb871f7ecb8726475d456a6ec5f))
* docker publish CLI verification (entrypoint + version grep) ([937f547](https://github.com/heiervang-technologies/unleash/commit/937f547f832be64891a98a7831fb7256e3c4c8f0))
* fall through to GitHub releases when npm unavailable for version check ([827b256](https://github.com/heiervang-technologies/unleash/commit/827b2568e523efbfc621460c84911041a659eb6d))
* guard -m/-p/-e parsers against consuming following flags as values ([0f66548](https://github.com/heiervang-technologies/unleash/commit/0f66548b02209b6ab9ed16bd0d0ba435961dd158))
* handle missing npm gracefully in agent install/update/uninstall ([fdf0bbf](https://github.com/heiervang-technologies/unleash/commit/fdf0bbf44a1e11369e82f3ccf8a96978fb01ef8b))
* headless prompt ignored when resume/continue is also set ([0afe2c5](https://github.com/heiervang-technologies/unleash/commit/0afe2c5f11f71e2ac76cf06b4579becaebb3d23e))
* install_state never set in npm-first install path ([ed6248e](https://github.com/heiervang-technologies/unleash/commit/ed6248ea5da013876d54677ec7b12227666b5a15))
* install/uninstall subcommands bypass wrapper reentry path ([cd6fc1b](https://github.com/heiervang-technologies/unleash/commit/cd6fc1b028e8d059dc9bb3c5324edf628c40719f))
* **interchange:** correct gemini tool result extraction test assertion ([f80ed18](https://github.com/heiervang-technologies/unleash/commit/f80ed1838117da57926ffc3243eb7d387cbf4587))
* **interchange:** correct gemini tool result extraction test assertion ([7fa4437](https://github.com/heiervang-technologies/unleash/commit/7fa4437401289e65286b0b90cbd0b2e2a60c9de4))
* **interchange:** correct gemini tool_call_with_result test assertions ([7ee7fa2](https://github.com/heiervang-technologies/unleash/commit/7ee7fa22f87c896b6524c03a5b8ceb638de75c8d)), closes [#56](https://github.com/heiervang-technologies/unleash/issues/56)
* **interchange:** improve cross-cli history portability and tests ([2dd58a4](https://github.com/heiervang-technologies/unleash/commit/2dd58a4d9c38c1d3ae1ee31757e96f1431b83184))
* **interchange:** lossless round-trip for Claude converter ([2afc0c9](https://github.com/heiervang-technologies/unleash/commit/2afc0c980b7db0f71d39aeaa2ada82fa62c36c4f))
* **interchange:** lossless round-trip for Codex converter ([90ed203](https://github.com/heiervang-technologies/unleash/commit/90ed20329d73a367ac486fa5509c26d98eb5e67f))
* **interchange:** lossless round-trip for Gemini converter ([5e8600e](https://github.com/heiervang-technologies/unleash/commit/5e8600e6ac404cc04bc9d9896283883c0fbd08a6))
* **interchange:** lossless round-trip for OpenCode converter ([3fd0b89](https://github.com/heiervang-technologies/unleash/commit/3fd0b890933d6831b8afce8bfa20ef1f60275cba))
* **interchange:** preserve images through OpenCode via _hub_images extension ([367df97](https://github.com/heiervang-technologies/unleash/commit/367df972e2f7bbf9ba497116fe9ee2efbcd80678))
* **interchange:** resolve claude/gemini cross-cli portability issues ([83fcec6](https://github.com/heiervang-technologies/unleash/commit/83fcec62cc6afc97191703e10e23a8d0cb315f89))
* npm prompt only in CLI mode, clear error in TUI ([9d949cb](https://github.com/heiervang-technologies/unleash/commit/9d949cb15bdbce0058132356b6888cdbfcea3051))
* parse_wrapper_launch_args does not consume flag as -p/-prompt value ([cdcf87b](https://github.com/heiervang-technologies/unleash/commit/cdcf87bd09002e34dc1e91e4c7c8db69559d3396))
* prevent test_install_version from overwriting real Claude installation ([886949b](https://github.com/heiervang-technologies/unleash/commit/886949b189f6eb2b774ff87a8689db0ca955b51c))
* prevent TUI install tests from downloading real agent binaries ([063ffa6](https://github.com/heiervang-technologies/unleash/commit/063ffa6ce5bad572717de66112187f4cd52a413e))
* remove stray eprintln! calls in install_codex_binary() that corrupt progress display ([f855c0b](https://github.com/heiervang-technologies/unleash/commit/f855c0b63b8f5a640197f2384b7465aba5ac77bb))
* remove stray eprintln! in update_codex and use npm_global_command for uninstall ([331e7cf](https://github.com/heiervang-technologies/unleash/commit/331e7cf7cad667053f8b207e1b24ba8574fcf201))
* reserved profile names, hooks error message, crossload, and restart handler ([109993b](https://github.com/heiervang-technologies/unleash/commit/109993bc79340b27b727214601186cb5f57ff1d6))
* respect CODEX_HOME env var in Codex session discovery and injection ([b892cc7](https://github.com/heiervang-technologies/unleash/commit/b892cc77f56c329829b8e27f479405cd85278f35))
* **sandbox:** status works without docker dir ([#73](https://github.com/heiervang-technologies/unleash/issues/73)) ([482d80f](https://github.com/heiervang-technologies/unleash/commit/482d80f47a27ad36b5a66f66b60e0e8a798328ef))
* session discovery, interchange layer, and cross-platform sha256 ([c846ff0](https://github.com/heiervang-technologies/unleash/commit/c846ff0aebf32fa8767cc63e0841952226483c54))
* sessions subcommand ignores --json flag ([5f777b5](https://github.com/heiervang-technologies/unleash/commit/5f777b5072c89cdf3034ef48621b2e0dfd9c93f2))
* sha256_hex falls back to shasum -a 256 on macOS ([26c3569](https://github.com/heiervang-technologies/unleash/commit/26c3569236e9b583a643378e9ad8daf4d4f385df))
* supercompact reliability and refresh loop ([#54](https://github.com/heiervang-technologies/unleash/issues/54)) ([7b6a211](https://github.com/heiervang-technologies/unleash/commit/7b6a211a871ddeb59b42af4324f78cb18e6bdd15))
* sync plugin hooks into settings.json on every launch ([#74](https://github.com/heiervang-technologies/unleash/issues/74)) ([b4a15ed](https://github.com/heiervang-technologies/unleash/commit/b4a15ed9431a78b42ef67d702256d2a917f264f7))
* three bugs in interchange/sessions.rs ([76274a2](https://github.com/heiervang-technologies/unleash/commit/76274a224077faa8429ab8992403d19f3c163bfd))
* **tui:** sync plugin hooks when toggling features ([79b0dc6](https://github.com/heiervang-technologies/unleash/commit/79b0dc60aa2d2a62e690e65fdbab1c9799f1752d))
* **ucf:** cherry-pick session fixes from stale [#55](https://github.com/heiervang-technologies/unleash/issues/55) branch ([99dfa27](https://github.com/heiervang-technologies/unleash/commit/99dfa2761cc7eccde52cb2b09c5c84464f6eee07))
* **ucf:** session discovery, UUID validation, and init message ([851cd5d](https://github.com/heiervang-technologies/unleash/commit/851cd5d695b2bae9d745dc99fb20c188d4342249))
* unleash agents check (no arg) now checks all four agents ([62e2551](https://github.com/heiervang-technologies/unleash/commit/62e2551207804ef13f62fc23a8e75d77ec49a7d1))
* update stale default model in restart-handler.sh to claude-sonnet-4-6 ([56038e2](https://github.com/heiervang-technologies/unleash/commit/56038e22a6d88ca0f055f5b200bbaa41e202bf0e))
* use --entrypoint for gh version check in docker publish ([2160248](https://github.com/heiervang-technologies/unleash/commit/2160248cb25dbe4ef49b999c7c502f810203555a))
* use cross for aarch64-linux-musl builds (sqlite3 compat) ([0728d03](https://github.com/heiervang-technologies/unleash/commit/0728d03f62e71747bd47fc1638ec6d78efc46e5b))
* use musl targets for Linux builds (static binaries, no glibc dep) ([94663d3](https://github.com/heiervang-technologies/unleash/commit/94663d3d4836c8b32caaddbc8d9fed6a2d3819fd))
* use npm_global_command() in install_version fallback paths ([512af95](https://github.com/heiervang-technologies/unleash/commit/512af95eccde29eef7bbbb1cba8641c9d88df776))
* use npm_global_command() in update_claude() for consistent sudo handling ([a6c5dff](https://github.com/heiervang-technologies/unleash/commit/a6c5dff830c51fb7f958cb3272e4146c17071ec9))


### Performance Improvements

* avoid reading input file twice for gemini conversion ([4697143](https://github.com/heiervang-technologies/unleash/commit/4697143e3b43da1575bfcf5efb1cd25101849be3))

## [0.1.1](https://github.com/heiervang-technologies/unleash/compare/v0.1.0...v0.1.1) (2026-03-30)


### Bug Fixes

* installer downloads prebuilt binaries, no cargo required ([c1791d9](https://github.com/heiervang-technologies/unleash/commit/c1791d90e3c5537906a5b66829d0843be6f44538))

## 0.1.0 (2026-03-30)


### Features

* multi-platform release binaries + interactive installer ([6baf1d3](https://github.com/heiervang-technologies/unleash/commit/6baf1d3504209a82b915d187d69193edcc2663a8))
* multi-platform splash binaries + installer downloads both ([58032b6](https://github.com/heiervang-technologies/unleash/commit/58032b6ae4ac5a5a8cb5b27236bb8575bb7f2244))

## 1.0.0

Initial open-source release.

### Features

- **Unified CLI wrapper** for Claude Code, Codex, Gemini CLI, and OpenCode
- **Polyfill flag layer** — common flags (`-m`, `-p`, `-c`, `-r`, `-e`, `-a`, `--safe`) work across all agents
- **TUI** for profile management, agent version control, and session browsing
- **Profile system** — named TOML configurations with per-profile model, effort, and safe-mode defaults
- **Agent lifecycle management** — `unleash install`, `unleash update`, `unleash uninstall`
- **Crossload** — portable conversation histories between CLI formats (Claude, Codex, Gemini, OpenCode)
- **Interactive installer** with ANSI mascot art and agent-specific theme recoloring
- **Plugin system** with bundled plugins:
  - **auto-mode** — autonomous operation between prompts via Stop hook
  - **process-restart** — self-restart with session preservation (`unleash-refresh`)
  - **mcp-refresh** — detect and reload MCP config changes
  - **hyprland-focus** — window transparency on Hyprland
- **Docker support** — sandboxed containers and multi-agent mesh
- **Diagonal gradient theming** — per-agent mascot art recoloring (e.g., Gemini blue-to-pink gradient)
- **Multi-platform binaries** — Linux x86_64/aarch64, macOS x86_64/aarch64
- **Yolo mode** by default — permission prompts bypassed, `--safe` to restore
