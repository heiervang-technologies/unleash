# Changelog

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
