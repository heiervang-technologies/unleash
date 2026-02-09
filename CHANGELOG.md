# Changelog

## [5.5.0](https://github.com/heiervang-technologies/agent-unleashed/compare/v5.4.0...v5.5.0) (2026-02-09)


### Features

* whitelist Claude Code v2.1.37 ([#124](https://github.com/heiervang-technologies/agent-unleashed/issues/124)) ([c7293ab](https://github.com/heiervang-technologies/agent-unleashed/commit/c7293ab83f59bb38391524c5145c31d704cef14b))

## [5.4.0](https://github.com/heiervang-technologies/agent-unleashed/compare/v5.3.0...v5.4.0) (2026-02-08)


### Features

* add local and pony codex profiles ([be4aa75](https://github.com/heiervang-technologies/agent-unleashed/commit/be4aa754d669236c355f479408c62d3b27ee7a42))

## [5.3.0](https://github.com/heiervang-technologies/agent-unleashed/compare/v5.2.0...v5.3.0) (2026-02-06)


### Features

* add Codex version management with whitelist/blacklist filtering ([#117](https://github.com/heiervang-technologies/agent-unleashed/issues/117)) ([05d10f8](https://github.com/heiervang-technologies/agent-unleashed/commit/05d10f8faa67e21cf02b59d47a36e5e6bd3aa162))

## [5.2.0](https://github.com/heiervang-technologies/agent-unleashed/compare/v5.1.1...v5.2.0) (2026-02-06)


### Features

* whitelist Claude Code v2.1.32 and add whitelisting requirements doc ([#119](https://github.com/heiervang-technologies/agent-unleashed/issues/119)) ([a27dfdf](https://github.com/heiervang-technologies/agent-unleashed/commit/a27dfdf96492ce1d6f514a405cda842137f6c043))

## [5.1.1](https://github.com/heiervang-technologies/agent-unleashed/compare/v5.1.0...v5.1.1) (2026-02-02)


### Bug Fixes

* handle multiline TOML stop_prompt in auto-mode hook ([#115](https://github.com/heiervang-technologies/agent-unleashed/issues/115)) ([8ac908b](https://github.com/heiervang-technologies/agent-unleashed/commit/8ac908ba0f7f11fee845b11bffccb7bad7d39745))

## [5.1.0](https://github.com/heiervang-technologies/agent-unleashed/compare/v5.0.2...v5.1.0) (2026-02-02)


### Features

* add Claude Code 2.1.29 patch support and fix stop prompt handling ([#113](https://github.com/heiervang-technologies/agent-unleashed/issues/113)) ([acd736a](https://github.com/heiervang-technologies/agent-unleashed/commit/acd736a46f384e52d0e7ac34260a859a1a16fcb3))

## [5.0.2](https://github.com/heiervang-technologies/agent-unleashed/compare/v5.0.1...v5.0.2) (2026-02-02)


### Bug Fixes

* point claude symlink to npm cli.js instead of managed binary ([#111](https://github.com/heiervang-technologies/agent-unleashed/issues/111)) ([e1b06f2](https://github.com/heiervang-technologies/agent-unleashed/commit/e1b06f23462797579eb690399d21a34282675373))

## [5.0.1](https://github.com/heiervang-technologies/agent-unleashed/compare/v5.0.0...v5.0.1) (2026-01-29)


### Bug Fixes

* **ci:** install clang/mold for Rust builds and rewrite upstream sync ([7b0582f](https://github.com/heiervang-technologies/agent-unleashed/commit/7b0582f95a5011cbefdce4b5517f45af063c3bf9))

## [5.0.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.10.0...v5.0.0) (2026-01-29)


### ⚠ BREAKING CHANGES

* Project renamed from claude-unleashed to agent-unleashed

### Features

* **v5**: Agent unleashed - agent agnostic wrapper ([#82](https://github.com/heiervang-technologies/claude-unleashed/issues/82)) ([dd97f2d](https://github.com/heiervang-technologies/claude-unleashed/commit/dd97f2da8b70a04852fa6d9f80ca008b3ea4f976))

## [4.10.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.9.0...v4.10.0) (2026-01-28)


### Features

* **hooks:** add centralized hook management system ([caf2e83](https://github.com/heiervang-technologies/claude-unleashed/commit/caf2e83c663580e371b3dec15d8fb89e44c4121c))


### Bug Fixes

* **hooks:** don't sync plugin hooks to settings.json ([fa80ba6](https://github.com/heiervang-technologies/claude-unleashed/commit/fa80ba6eab2a04e3736a8e3e0c852949bc6aaa5e))
* **plugins:** deduplicate plugin directories ([86cd7b8](https://github.com/heiervang-technologies/claude-unleashed/commit/86cd7b82f6df85f5edaf4399ec1f4f17c80c6ada))

## [4.9.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.8.0...v4.9.0) (2026-01-28)


### Features

* **auto-mode:** improve startup behavior and add env var patches ([ab32ef1](https://github.com/heiervang-technologies/claude-unleashed/commit/ab32ef17c88bf873d16dd8509d76453d8ddfddcf))

## [4.8.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.7.0...v4.8.0) (2026-01-28)


### Features

* **patches:** add support for Claude Code v2.1.22 ([a88d38d](https://github.com/heiervang-technologies/claude-unleashed/commit/a88d38d5c41eef2bb3d83e8ba4503716449b8250))


### Bug Fixes

* **patches:** add auto mode to validation function ([dc43e0d](https://github.com/heiervang-technologies/claude-unleashed/commit/dc43e0dee998726b5f5c9a4a1d85ef3a499d7b32))

## [4.7.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.6.1...v4.7.0) (2026-01-23)


### Features

* **plugins:** add omnihook plugin for low-latency voice integration ([8eb5dc0](https://github.com/heiervang-technologies/claude-unleashed/commit/8eb5dc0137081c32be59147325bc68e9f9cfbc63))
* **tui:** add viewport scrolling and external editor for stop-prompt ([7393f70](https://github.com/heiervang-technologies/claude-unleashed/commit/7393f70785ab9386945edd964591e0099907d127))

## [4.6.1](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.6.0...v4.6.1) (2026-01-21)


### Bug Fixes

* add auto-onboard bypass and whitelist version 2.1.14 ([a53e720](https://github.com/heiervang-technologies/claude-unleashed/commit/a53e720d7ba41fed9c4284e6711ede81762d796c)), closes [#83](https://github.com/heiervang-technologies/claude-unleashed/issues/83)
* remove 2.1.14 from whitelist ([0f4c2eb](https://github.com/heiervang-technologies/claude-unleashed/commit/0f4c2eb8dee77b29f67c1a111df0f71080df22df))

## [4.6.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.5.2...v4.6.0) (2026-01-21)


### Features

* Add OpenRouter configuration for Codex CLI ([c7ecdb0](https://github.com/heiervang-technologies/claude-unleashed/commit/c7ecdb009c1194ee4fd48512087517b9c52c584b))

## [4.5.2](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.5.1...v4.5.2) (2026-01-21)


### Bug Fixes

* update TUI update logic, documentation paths, and install scripts ([b88220e](https://github.com/heiervang-technologies/claude-unleashed/commit/b88220e4a09abb15c3f6e11591fd2e665c453c36))
* update TUI update logic, documentation paths, and install scripts ([7a1f78c](https://github.com/heiervang-technologies/claude-unleashed/commit/7a1f78c7a8a4a350ae4d9d8a247474c098ec993a))

## [4.5.1](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.5.0...v4.5.1) (2026-01-19)


### Bug Fixes

* use Zot registry instead of Docker Hub for snail image ([#75](https://github.com/heiervang-technologies/claude-unleashed/issues/75)) ([23ca208](https://github.com/heiervang-technologies/claude-unleashed/commit/23ca208a348a14ab90672ef7735c714d7702ba64))

## [4.5.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.4.0...v4.5.0) (2026-01-17)


### Features

* Add cutx go command and cutxg shorthand ([1da5a46](https://github.com/heiervang-technologies/claude-unleashed/commit/1da5a460074f0fb265af699123192d257010cb32))

## [4.4.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.3.0...v4.4.0) (2026-01-17)


### Features

* Update README with animation demo GIF ([73c6126](https://github.com/heiervang-technologies/claude-unleashed/commit/73c61260ecffe5173f82e5f7ad7a85bbd8c7891a))


### Bug Fixes

* Async version loading and settings text truncation ([6bed2df](https://github.com/heiervang-technologies/claude-unleashed/commit/6bed2df607feeab6a8095d7273c7ba0e3840c9eb))

## [4.3.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.2.0...v4.3.0) (2026-01-17)


### Features

* add smooth slide animation for art sidebar ([#65](https://github.com/heiervang-technologies/claude-unleashed/issues/65)) ([ea2841d](https://github.com/heiervang-technologies/claude-unleashed/commit/ea2841daf79f4629b6834cc0f0a9ee1d3bb88361))

## [4.2.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.1.2...v4.2.0) (2026-01-17)


### Features

* add patch config for Claude Code 2.1.12 and stop hook integration tests ([#67](https://github.com/heiervang-technologies/claude-unleashed/issues/67)) ([1e769ec](https://github.com/heiervang-technologies/claude-unleashed/commit/1e769ec316fcd84e91be5d949fcdcd6618dad28e))

## [4.1.2](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.1.1...v4.1.2) (2026-01-17)


### Bug Fixes

* add --force flag to npm install for version downgrades ([#60](https://github.com/heiervang-technologies/claude-unleashed/issues/60)) ([e30a1f5](https://github.com/heiervang-technologies/claude-unleashed/commit/e30a1f53eedf71ea1bf43fda007e5bc5e4d91aab))

## [4.1.1](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.1.0...v4.1.1) (2026-01-17)


### Bug Fixes

* address code review issues ([#61](https://github.com/heiervang-technologies/claude-unleashed/issues/61)) ([0a27c3b](https://github.com/heiervang-technologies/claude-unleashed/commit/0a27c3b47a45f1a833f710b33084cf8d6e439fff))

## [4.1.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.0.0...v4.1.0) (2026-01-17)


### Features

* add muscular Claude artwork to TUI and installer ([#56](https://github.com/heiervang-technologies/claude-unleashed/issues/56)) ([48e2bd4](https://github.com/heiervang-technologies/claude-unleashed/commit/48e2bd40c99e8cf9e4df606673733883685d2b5c))

## [4.0.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v3.0.0...v4.0.0) (2026-01-17)


### ⚠ BREAKING CHANGES

* The blacklist system has been replaced with a whitelist system. Only whitelisted versions will be installed when using "latest".

### Features

* change blacklist system to whitelist system ([#53](https://github.com/heiervang-technologies/claude-unleashed/issues/53)) ([ad51a07](https://github.com/heiervang-technologies/claude-unleashed/commit/ad51a073a69b9ebd52ebcb0c720bc6c437b49a12))

## [3.0.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v2.8.0...v3.0.0) (2026-01-16)


### ⚠ BREAKING CHANGES

* The `cuw` command has been removed. Use `cu go` or `cug` instead.

### Features

* remove cuw, use multiple cargo binaries for all commands ([#51](https://github.com/heiervang-technologies/claude-unleashed/issues/51)) ([2a3fc24](https://github.com/heiervang-technologies/claude-unleashed/commit/2a3fc24cb62275c74e079a9f0ed32b114a34d9c2))

## [2.8.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v2.7.0...v2.8.0) (2026-01-13)


### Features

* use self-hosted runners for all GitHub Actions workflows ([#47](https://github.com/heiervang-technologies/claude-unleashed/issues/47)) ([65ad326](https://github.com/heiervang-technologies/claude-unleashed/commit/65ad326c3fe6afbd05532fa087a896cebf011153))

## [2.7.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v2.6.0...v2.7.0) (2026-01-13)


### Features

* add defensive TTY check before TUI initialization ([06dbe90](https://github.com/heiervang-technologies/claude-unleashed/commit/06dbe904fe4ec27a2bc45c1731d42bd082609ea8))

## [2.6.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v2.5.0...v2.6.0) (2026-01-13)


### Features

* make TUI optional for headless environments ([d12eb1e](https://github.com/heiervang-technologies/claude-unleashed/commit/d12eb1ec267cf36f11c4aab55721b9c5348983c9))

## [2.5.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v2.4.0...v2.5.0) (2026-01-13)


### Features

* make install runs full script, add uninstall command ([1223f37](https://github.com/heiervang-technologies/claude-unleashed/commit/1223f37ba3c4ccc25c4f7c613ace8084d6878ac1))


### Bug Fixes

* auto-merge should run whenever PR exists ([9f919be](https://github.com/heiervang-technologies/claude-unleashed/commit/9f919be73ffbdc56eb7ba4bc9e4cb22c9957d0b4))

## [2.4.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v2.3.1...v2.4.0) (2026-01-13)


### Features

* auto-merge minor/patch releases, require PR for major ([acf3695](https://github.com/heiervang-technologies/claude-unleashed/commit/acf36959cf29899fb34e541172dadf1cebd70659))

## [2.3.1](https://github.com/heiervang-technologies/claude-unleashed/compare/v2.3.0...v2.3.1) (2026-01-13)


### Bug Fixes

* Esc is Back, Back on main menu quits ([0a56eb5](https://github.com/heiervang-technologies/claude-unleashed/commit/0a56eb5065cca77e34c53ae05a3e17fb9a409523))

## [2.3.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v2.2.0...v2.3.0) (2026-01-13)


### Features

* restructure CLI with subcommands ([0305ba7](https://github.com/heiervang-technologies/claude-unleashed/commit/0305ba7d5f7e0b5ac3d0829ffc99462ba39378cf))

## [2.2.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v2.1.1...v2.2.0) (2026-01-13)


### Features

* add 2.1.4 patch config and improve TUI text input ([02c6c4b](https://github.com/heiervang-technologies/claude-unleashed/commit/02c6c4bfb7dc7fbd955662b879ce8380e4d1559b))
* add comprehensive CLI improvements to cu ([#34](https://github.com/heiervang-technologies/claude-unleashed/issues/34)) ([18f3aa6](https://github.com/heiervang-technologies/claude-unleashed/commit/18f3aa6a320f28f1f97bf7532a6fb1060f4b8b27))


### Bug Fixes

* allow Escape to quit/exit in addition to q ([1b0b5a7](https://github.com/heiervang-technologies/claude-unleashed/commit/1b0b5a71e903c3cf31519c9c25e018b2a810053f))
* install plugins globally for /auto command to work everywhere ([#35](https://github.com/heiervang-technologies/claude-unleashed/issues/35)) ([275a53e](https://github.com/heiervang-technologies/claude-unleashed/commit/275a53e26bff45b85f60672e593456b76c90f111))
* pre-populate stop prompt field with default message ([84c8d1d](https://github.com/heiervang-technologies/claude-unleashed/commit/84c8d1de743d40dc16928267afa5b896968fbda8))
* read default stop prompt from hook script (source of truth) ([60c1a82](https://github.com/heiervang-technologies/claude-unleashed/commit/60c1a8298c93a5b9d4a4b31c685d5deadf4d64a2))
* restore comprehensive default stop prompt for auto-mode ([c437ee1](https://github.com/heiervang-technologies/claude-unleashed/commit/c437ee1d7adb26a74c9f70ee7cd6fe0d3bb62f6a))
* use reusable mention-trigger workflow from core ([#32](https://github.com/heiervang-technologies/claude-unleashed/issues/32)) ([2457e50](https://github.com/heiervang-technologies/claude-unleashed/commit/2457e50966152244b211da74d71d5e978189bcb1))

## [2.1.1](https://github.com/heiervang-technologies/claude-unleashed/compare/v2.1.0...v2.1.1) (2026-01-12)


### Bug Fixes

* add trailing newline before reversing version list ([27613a1](https://github.com/heiervang-technologies/claude-unleashed/commit/27613a111d8621af72623651a051df30bc5444f3))

## [2.1.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v2.0.0...v2.1.0) (2026-01-12)


### Features

* add version blacklist for Claude Code installations ([#26](https://github.com/heiervang-technologies/claude-unleashed/issues/26)) ([e791193](https://github.com/heiervang-technologies/claude-unleashed/commit/e7911934ad167e018c95a6292e5bbd434355f443))


### Bug Fixes

* cache installed version to avoid subprocess on every TUI frame ([#25](https://github.com/heiervang-technologies/claude-unleashed/issues/25)) ([f403428](https://github.com/heiervang-technologies/claude-unleashed/commit/f40342831d25fb8cf74b0b71d6ddc67ddc369daf))

## [Unreleased]

### Features

* **config**: add configurable stop-hook prompts
  - CLI flags: `--stop-prompt`, `--stop-prompt-edit`, `--stop-prompt-clear`
  - TUI settings screen for stop prompt configuration
  - Global config storage in `~/.config/claude-unleashed/config.toml`
  - Three-tier priority: session-specific > global config > default
  - Documentation in `docs/extensions/configuration.md`

## [2.0.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v1.0.0...v2.0.0) (2026-01-12)


### ⚠ BREAKING CHANGES

* Binary renamed from `cui` to `cu`. The `cui` and `cutx` commands are now symlinks to the main `cu` binary.

### Features

* add auto mode patch for Claude Code 2.1.5 ([a37dedf](https://github.com/heiervang-technologies/claude-unleashed/commit/a37dedf6789e1139e54d8bc81a1285ae81084301))
* add installation scripts and TUI version management ([83f4121](https://github.com/heiervang-technologies/claude-unleashed/commit/83f4121ea9390a52a7b88b1b0e071dcc2f320211))
* add private repo support for installation ([a20a92e](https://github.com/heiervang-technologies/claude-unleashed/commit/a20a92e9d4a181e79f08ce7f6b1b4e33e06493c8))
* unify cu, cui, cutx into single binary ([3d23a33](https://github.com/heiervang-technologies/claude-unleashed/commit/3d23a338d9683469969c5be81be25061391ed723))


### Bug Fixes

* align binary artifact names with release workflow ([41556c7](https://github.com/heiervang-technologies/claude-unleashed/commit/41556c7a9ed030677c2bba017332f8cabd927e7f))
* improve README structure and installer update logic ([16494c6](https://github.com/heiervang-technologies/claude-unleashed/commit/16494c60dc47ae4e4844ec4ad2b25e07b28b5a79))
* remove unused TARGET variable in install script ([a59324d](https://github.com/heiervang-technologies/claude-unleashed/commit/a59324dd152e78c1fa8444d23675a1e9b6aaa969))
* support GH_PAT and GITHUB_TOKEN in addition to GH_TOKEN ([4f4dfde](https://github.com/heiervang-technologies/claude-unleashed/commit/4f4dfde99b3a4986dd7a5c08ab8e82828e374cb0))
* suppress 404 errors when binary not available ([39e9d2f](https://github.com/heiervang-technologies/claude-unleashed/commit/39e9d2f73886e63c1ffa610931275c9f3266b562))
* use gh cli for binary downloads from private repos ([8a649cb](https://github.com/heiervang-technologies/claude-unleashed/commit/8a649cb6d745a4d089beb17fee9cbcc88afa6930))

## [1.0.0-91f740b](https://github.com/heiervang-technologies/claude-unleashed/compare/v0.1.0-91f740b...v1.0.0-91f740b) (2026-01-11)


### ⚠ BREAKING CHANGES

* TUI binary renamed from claude-unleashed to cui

### Code Refactoring

* unify CLI entry points as cu/cui/cutx ([#19](https://github.com/heiervang-technologies/claude-unleashed/issues/19)) ([e3728ce](https://github.com/heiervang-technologies/claude-unleashed/commit/e3728ce5bb245efde5f810042a3b28d1db598ccd))
