# Changelog

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
