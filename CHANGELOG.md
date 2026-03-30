# Changelog

## [9.14.2](https://github.com/heiervang-technologies/unleash/compare/v9.14.1...v9.14.2) (2026-03-30)


### Bug Fixes

* TUI launch sets AGENT_UNLEASH so child enters wrapper mode ([#309](https://github.com/heiervang-technologies/unleash/issues/309)) ([768eae1](https://github.com/heiervang-technologies/unleash/commit/768eae17c48f4b1dd9e09fc5fcd475f1d8b77b2e))

## [9.14.1](https://github.com/heiervang-technologies/unleash/compare/v9.14.0...v9.14.1) (2026-03-30)


### Bug Fixes

* Codex crossload resume — originator, null payloads, state DB registration ([6c36d42](https://github.com/heiervang-technologies/unleash/commit/6c36d42766c29b604396c69e0b80ff064bc6c3af))

## [9.14.0](https://github.com/heiervang-technologies/unleash/compare/v9.13.1...v9.14.0) (2026-03-30)


### Features

* filter non-message events from Claude crossload injection ([b8faac6](https://github.com/heiervang-technologies/unleash/commit/b8faac6abaf8b517a1a0305d9492c6d3ab90a353))


### Bug Fixes

* resolve all clippy warnings, fix installer alignment, lowercase branding ([#302](https://github.com/heiervang-technologies/unleash/issues/302)) ([e13068f](https://github.com/heiervang-technologies/unleash/commit/e13068f56813de2af5cde2d4a5fb2ec660c16275))

## [9.13.1](https://github.com/heiervang-technologies/unleash/compare/v9.13.0...v9.13.1) (2026-03-30)


### Bug Fixes

* strip profile name from wrapper crossload args ([c29c1aa](https://github.com/heiervang-technologies/unleash/commit/c29c1aa9b60f184887ab3313a8d98b00f378b987))

## [9.13.0](https://github.com/heiervang-technologies/unleash/compare/v9.12.2...v9.13.0) (2026-03-30)


### Features

* fix Codex converter deduplication and developer role filtering ([2782166](https://github.com/heiervang-technologies/unleash/commit/27821664ef20dcf542d3120dd7f27783e9b18f28))

## [9.12.2](https://github.com/heiervang-technologies/unleash/compare/v9.12.1...v9.12.2) (2026-03-30)


### Bug Fixes

* crossload target detection uses profile name not AGENT_CMD ([deb5aab](https://github.com/heiervang-technologies/unleash/commit/deb5aaba849c3dcf712b70d63c38a69680a456db))

## [9.12.1](https://github.com/heiervang-technologies/unleash/compare/v9.12.0...v9.12.1) (2026-03-30)


### Bug Fixes

* Gemini session gets valid startTime and message IDs ([9e5e888](https://github.com/heiervang-technologies/unleash/commit/9e5e888f1e7dfa9c7a7471ad37dc52308225bcaa))

## [9.12.0](https://github.com/heiervang-technologies/unleash/compare/v9.11.10...v9.12.0) (2026-03-30)


### Features

* Gemini session format fix + cross-CLI test coverage ([#297](https://github.com/heiervang-technologies/unleash/issues/297)) ([ca97f49](https://github.com/heiervang-technologies/unleash/commit/ca97f497ca08944c248da094ec94556adbddefe6))


### Bug Fixes

* Gemini injection uses project slug not SHA-256 hash for directory ([42f1492](https://github.com/heiervang-technologies/unleash/commit/42f1492af3a746c608211ebf3d77b7acf98369d4))

## [9.11.10](https://github.com/heiervang-technologies/unleash/compare/v9.11.9...v9.11.10) (2026-03-29)


### Bug Fixes

* only add cross-CLI defaults when source is not Claude ([4e4b7c4](https://github.com/heiervang-technologies/unleash/commit/4e4b7c49458d401e1d94ff0c43544cb3a11dfc31))

## [9.11.9](https://github.com/heiervang-technologies/unleash/compare/v9.11.8...v9.11.9) (2026-03-29)


### Bug Fixes

* convert foreign thinking blocks to text for Claude injection ([af07d78](https://github.com/heiervang-technologies/unleash/commit/af07d78886b7b5fe6b098fb4876a003372ab1fd2))

## [9.11.8](https://github.com/heiervang-technologies/unleash/compare/v9.11.7...v9.11.8) (2026-03-29)


### Bug Fixes

* Gemini injection uses project slug from projects.json ([366a9fd](https://github.com/heiervang-technologies/unleash/commit/366a9fdc4a0abc6de21b96cd4640b3ce3cf532d3))

## [9.11.7](https://github.com/heiervang-technologies/unleash/compare/v9.11.6...v9.11.7) (2026-03-29)


### Bug Fixes

* Gemini injection uses SHA-256 project hash for correct directory ([217af52](https://github.com/heiervang-technologies/unleash/commit/217af52bcbada0df743c8dd868496a4238c74d9e))

## [9.11.6](https://github.com/heiervang-technologies/unleash/compare/v9.11.5...v9.11.6) (2026-03-29)


### Bug Fixes

* ensure every injected line has UUID and unbroken parent chain ([0698c0a](https://github.com/heiervang-technologies/unleash/commit/0698c0a108f9fee4aa7dac22778c0a35d4d8599b))

## [9.11.5](https://github.com/heiervang-technologies/unleash/compare/v9.11.4...v9.11.5) (2026-03-29)


### Bug Fixes

* build parentUuid chain for Claude session injection ([542d6a9](https://github.com/heiervang-technologies/unleash/commit/542d6a96f480484a3fdcd5b704e9d5c1ab5fcca9))

## [9.11.4](https://github.com/heiervang-technologies/unleash/compare/v9.11.3...v9.11.4) (2026-03-29)


### Bug Fixes

* Claude injection now generates all required fields for resume ([406710b](https://github.com/heiervang-technologies/unleash/commit/406710b10c9847edc7158bb02d1c5510ad9e86a4))

## [9.11.3](https://github.com/heiervang-technologies/unleash/compare/v9.11.2...v9.11.3) (2026-03-29)


### Bug Fixes

* handle --crossload/-x in wrapper reentry path ([7ac88a2](https://github.com/heiervang-technologies/unleash/commit/7ac88a27eff2f0f5384d523664c97b3c944aaa58))

## [9.11.2](https://github.com/heiervang-technologies/unleash/compare/v9.11.1...v9.11.2) (2026-03-29)


### Bug Fixes

* correct Codex session ID parsing and capture CWD/names ([9f0b0d3](https://github.com/heiervang-technologies/unleash/commit/9f0b0d3d5f8aeb3f535314d0b4cfdb3d738e1dcc))

## [9.11.1](https://github.com/heiervang-technologies/unleash/compare/v9.11.0...v9.11.1) (2026-03-29)


### Bug Fixes

* Claude crossload uses cwd for project path and fresh session ID ([230b405](https://github.com/heiervang-technologies/unleash/commit/230b4056ee222d191437f36ce07e71a3ee9f346e))

## [9.11.0](https://github.com/heiervang-technologies/unleash/compare/v9.10.3...v9.11.0) (2026-03-29)


### Features

* cross-CLI session resume with interactive picker ([#285](https://github.com/heiervang-technologies/unleash/issues/285)) ([1166b68](https://github.com/heiervang-technologies/unleash/commit/1166b68ee3292c5e52d07d52eb3e1da571291f94))

## [9.10.3](https://github.com/heiervang-technologies/unleash/compare/v9.10.2...v9.10.3) (2026-03-29)


### Performance Improvements

* eliminate blocking subprocess calls from startup path ([#282](https://github.com/heiervang-technologies/unleash/issues/282)) ([0ae3ca7](https://github.com/heiervang-technologies/unleash/commit/0ae3ca7400544bcd858ec703e757a0fc1894fca5)), closes [#209](https://github.com/heiervang-technologies/unleash/issues/209)

## [9.10.2](https://github.com/heiervang-technologies/unleash/compare/v9.10.1...v9.10.2) (2026-03-29)


### Bug Fixes

* confirm-delete uses wrong index when search filter is active ([#279](https://github.com/heiervang-technologies/unleash/issues/279)) ([70c01e4](https://github.com/heiervang-technologies/unleash/commit/70c01e416bcda6a85b7001281c6e2e4f64c7c9e8)), closes [#233](https://github.com/heiervang-technologies/unleash/issues/233)
* remove dead nohup spawn code, align trigger file paths ([#244](https://github.com/heiervang-technologies/unleash/issues/244)) ([#280](https://github.com/heiervang-technologies/unleash/issues/280)) ([970762b](https://github.com/heiervang-technologies/unleash/commit/970762bc6c56a1e1171bd94017022516ce9085b1))

## [9.10.1](https://github.com/heiervang-technologies/unleash/compare/v9.10.0...v9.10.1) (2026-03-29)


### Bug Fixes

* opencode update handles fresh install (ENOENT before npm fallback) ([#277](https://github.com/heiervang-technologies/unleash/issues/277)) ([0710a8b](https://github.com/heiervang-technologies/unleash/commit/0710a8b903d47ada339b9b71a3afcfade26dfb83)), closes [#266](https://github.com/heiervang-technologies/unleash/issues/266)

## [9.10.0](https://github.com/heiervang-technologies/unleash/compare/v9.9.0...v9.10.0) (2026-03-29)


### Features

* conversation hub interchange format with lossless converters ([#275](https://github.com/heiervang-technologies/unleash/issues/275)) ([61806c6](https://github.com/heiervang-technologies/unleash/commit/61806c6236e5eb4ed01474cc5abf0973d4418b5c))

## [9.9.0](https://github.com/heiervang-technologies/unleash/compare/v9.8.0...v9.9.0) (2026-03-29)


### Features

* multi-agent mesh networking with tiered isolation ([#271](https://github.com/heiervang-technologies/unleash/issues/271)) ([76c0bc8](https://github.com/heiervang-technologies/unleash/commit/76c0bc8d2025a2298bccb45ccc3308ef6cb81355))

## [9.8.0](https://github.com/heiervang-technologies/unleash/compare/v9.7.1...v9.8.0) (2026-03-29)


### Features

* gVisor sandbox with LAN isolation ([#268](https://github.com/heiervang-technologies/unleash/issues/268)) ([0445542](https://github.com/heiervang-technologies/unleash/commit/0445542ec535f8ad25db431b99834f882c97ce2d))

## [9.7.1](https://github.com/heiervang-technologies/unleash/compare/v9.7.0...v9.7.1) (2026-03-28)


### Bug Fixes

* prepend sudo to npm global installs when prefix is root-owned ([abf069c](https://github.com/heiervang-technologies/unleash/commit/abf069c1de512deb31755c77a0e47f141f2e9ccc)), closes [#263](https://github.com/heiervang-technologies/unleash/issues/263)
* prepend sudo to npm global installs when prefix is root-owned ([#264](https://github.com/heiervang-technologies/unleash/issues/264)) ([abf069c](https://github.com/heiervang-technologies/unleash/commit/abf069c1de512deb31755c77a0e47f141f2e9ccc))

## [9.7.0](https://github.com/heiervang-technologies/unleash/compare/v9.6.0...v9.7.0) (2026-03-28)


### Features

* sandboxed Docker container with all 4 coder CLIs ([1d16fd6](https://github.com/heiervang-technologies/unleash/commit/1d16fd63fa5f6cc839ebcf7bce26946b3c221580))
* sandboxed Docker container with all 4 coder CLIs ([#185](https://github.com/heiervang-technologies/unleash/issues/185)) ([1d16fd6](https://github.com/heiervang-technologies/unleash/commit/1d16fd63fa5f6cc839ebcf7bce26946b3c221580))

## [9.6.0](https://github.com/heiervang-technologies/unleash/compare/v9.5.3...v9.6.0) (2026-03-28)


### Features

* config restructure, effort polyfill, version poll TTL ([#261](https://github.com/heiervang-technologies/unleash/issues/261)) ([509ce01](https://github.com/heiervang-technologies/unleash/commit/509ce017a2438a005563392b224a836a91253ca3))
* restructure profile config, add effort polyfill, version poll TTL ([509ce01](https://github.com/heiervang-technologies/unleash/commit/509ce017a2438a005563392b224a836a91253ca3))

## [9.5.3](https://github.com/heiervang-technologies/unleash/compare/v9.5.2...v9.5.3) (2026-03-28)


### Bug Fixes

* remove cosmetic auto-update feature (config, TUI toggle, launch check) ([#256](https://github.com/heiervang-technologies/unleash/issues/256)) ([a9f6217](https://github.com/heiervang-technologies/unleash/commit/a9f6217bfa9fdd0921fabe6f02d3fb46f9b851ce))

## [9.5.2](https://github.com/heiervang-technologies/unleash/compare/v9.5.1...v9.5.2) (2026-03-28)


### Bug Fixes

* update bugs, version sorting, animations, checksum, conflict cleanup ([#254](https://github.com/heiervang-technologies/unleash/issues/254)) ([42e0eb0](https://github.com/heiervang-technologies/unleash/commit/42e0eb081e6df806cc03d957be78d60c5e7e1f99))

## [9.5.1](https://github.com/heiervang-technologies/unleash/compare/v9.5.0...v9.5.1) (2026-03-27)


### Bug Fixes

* batch fix 12 bugs found during code review ([6038d6a](https://github.com/heiervang-technologies/unleash/commit/6038d6ad6c7e7a8d5bfd3ffa399bda1b3bf86184))
* batch fix 12 bugs found during code review ([#249](https://github.com/heiervang-technologies/unleash/issues/249)) ([6038d6a](https://github.com/heiervang-technologies/unleash/commit/6038d6ad6c7e7a8d5bfd3ffa399bda1b3bf86184))

## [9.5.0](https://github.com/heiervang-technologies/unleash/compare/v9.4.0...v9.5.0) (2026-03-27)


### Features

* --dry-run, flag validation, command rename, conflict detail, TUI refresh ([#229](https://github.com/heiervang-technologies/unleash/issues/229)) ([eb0bb49](https://github.com/heiervang-technologies/unleash/commit/eb0bb491f29498fd64957e55ecc12d6593732904))
* --dry-run, flag validation, command rename, conflict detail, TUI refresh, auth fix ([eb0bb49](https://github.com/heiervang-technologies/unleash/commit/eb0bb491f29498fd64957e55ecc12d6593732904))

## [9.4.0](https://github.com/heiervang-technologies/unleash/compare/v9.3.0...v9.4.0) (2026-03-27)


### Features

* redesign update command + block reserved profile names ([#224](https://github.com/heiervang-technologies/unleash/issues/224)) ([add0555](https://github.com/heiervang-technologies/unleash/commit/add0555ad9e3207fad62d155838b77bfcc650618))
* redesign update command + reserved profile names + fixes ([add0555](https://github.com/heiervang-technologies/unleash/commit/add0555ad9e3207fad62d155838b77bfcc650618))

## [9.3.0](https://github.com/heiervang-technologies/unleash/compare/v9.2.4...v9.3.0) (2026-03-27)


### Features

* unleash update with parallel progress bars ([#221](https://github.com/heiervang-technologies/unleash/issues/221)) ([8b8abe2](https://github.com/heiervang-technologies/unleash/commit/8b8abe2464a93c5273c7d3c53e97697683821d83))
* unleash update with parallel progress bars + codex prebuilt binaries ([8b8abe2](https://github.com/heiervang-technologies/unleash/commit/8b8abe2464a93c5273c7d3c53e97697683821d83)), closes [#220](https://github.com/heiervang-technologies/unleash/issues/220)

## [9.2.4](https://github.com/heiervang-technologies/unleash/compare/v9.2.3...v9.2.4) (2026-03-26)


### Bug Fixes

* prevent stale AGENT_CMD from hijacking profile selection ([b57fb31](https://github.com/heiervang-technologies/unleash/commit/b57fb3160508f3735738d5bfcda369ac733aee6f))

## [9.2.3](https://github.com/heiervang-technologies/unleash/compare/v9.2.2...v9.2.3) (2026-03-26)


### Bug Fixes

* send auth status messages to stderr instead of stdout ([63b136d](https://github.com/heiervang-technologies/unleash/commit/63b136d0cb23d8ab97da22897344bf24b9a77ad6))

## [9.2.2](https://github.com/heiervang-technologies/unleash/compare/v9.2.1...v9.2.2) (2026-03-26)


### Bug Fixes

* improve help text and fix --help interception ([51a397d](https://github.com/heiervang-technologies/unleash/commit/51a397d47f113f3e6055a10541ece48f9ad118b4))

## [9.2.1](https://github.com/heiervang-technologies/unleash/compare/v9.2.0...v9.2.1) (2026-03-26)


### Bug Fixes

* unleash -h shows own help instead of agent help when AGENT_CMD set ([f396994](https://github.com/heiervang-technologies/unleash/commit/f3969940ab9d23285bae52978a36a4ea41a134d9))

## [9.2.0](https://github.com/heiervang-technologies/unleash/compare/v9.1.2...v9.2.0) (2026-03-26)


### Features

* unified argument polyfill layer for agent CLIs ([113d5b3](https://github.com/heiervang-technologies/unleash/commit/113d5b3c31b78215400b3c1ae91463d0e0db8de5)), closes [#210](https://github.com/heiervang-technologies/unleash/issues/210)
* unified argument polyfill layer for agent CLIs ([#211](https://github.com/heiervang-technologies/unleash/issues/211)) ([113d5b3](https://github.com/heiervang-technologies/unleash/commit/113d5b3c31b78215400b3c1ae91463d0e0db8de5))

## [9.1.2](https://github.com/heiervang-technologies/unleash/compare/v9.1.1...v9.1.2) (2026-03-24)


### Bug Fixes

* text overflow and UTF-8 panic in profile edit TUI ([#207](https://github.com/heiervang-technologies/unleash/issues/207)) ([11953dd](https://github.com/heiervang-technologies/unleash/commit/11953dde6a8e0fa17e1cd535c225ed10f4131fd7))

## [9.1.1](https://github.com/heiervang-technologies/unleash/compare/v9.1.0...v9.1.1) (2026-03-18)


### Bug Fixes

* OpenCode installation and version management broken ([#199](https://github.com/heiervang-technologies/unleash/issues/199)) ([#205](https://github.com/heiervang-technologies/unleash/issues/205)) ([f122d7f](https://github.com/heiervang-technologies/unleash/commit/f122d7f2a020c3ecf025e74392583ef090070f4a))

## [9.1.0](https://github.com/heiervang-technologies/unleash/compare/v9.0.0...v9.1.0) (2026-03-18)


### Features

* scrollable profile list with search and duplicate ([#202](https://github.com/heiervang-technologies/unleash/issues/202)) ([#203](https://github.com/heiervang-technologies/unleash/issues/203)) ([b1eacd4](https://github.com/heiervang-technologies/unleash/commit/b1eacd48952b801526d4b4dda9a9a4044a16c4f4))

## [9.0.0](https://github.com/heiervang-technologies/unleash/compare/v8.5.3...v9.0.0) (2026-03-18)


### ⚠ BREAKING CHANGES

* drop unleashed/u binaries, consolidate to single unleash binary ([#200](https://github.com/heiervang-technologies/unleash/issues/200))

### Features

* drop unleashed/u binaries, consolidate to single unleash binary ([#200](https://github.com/heiervang-technologies/unleash/issues/200)) ([50b4614](https://github.com/heiervang-technologies/unleash/commit/50b461492aa59024facd40947124e70814cbc369))

## [8.5.3](https://github.com/heiervang-technologies/unleash/compare/v8.5.2...v8.5.3) (2026-03-17)


### Bug Fixes

* resolve docs and runtime drift ([#193](https://github.com/heiervang-technologies/unleash/issues/193)) ([#195](https://github.com/heiervang-technologies/unleash/issues/195)) ([f4666ab](https://github.com/heiervang-technologies/unleash/commit/f4666ab4a3cbb073c8648d52472913e39b51295c))

## [8.5.2](https://github.com/heiervang-technologies/unleash/compare/v8.5.1...v8.5.2) (2026-03-06)


### Bug Fixes

* make overflowing profile list scrollable in TUI ([#194](https://github.com/heiervang-technologies/unleash/issues/194)) ([17fcc7f](https://github.com/heiervang-technologies/unleash/commit/17fcc7f515db0e8c32a1ae0b051d10c619cedc8b)), closes [#192](https://github.com/heiervang-technologies/unleash/issues/192)

## [8.5.1](https://github.com/heiervang-technologies/unleash/compare/v8.5.0...v8.5.1) (2026-03-04)


### Bug Fixes

* proper cursor positioning in TUI text input fields ([#190](https://github.com/heiervang-technologies/unleash/issues/190)) ([88c9520](https://github.com/heiervang-technologies/unleash/commit/88c95201926024dd7f3a91d6094e46b6fe604adb))

## [8.5.0](https://github.com/heiervang-technologies/unleash/compare/v8.4.0...v8.5.0) (2026-03-01)


### Features

* extend hyprland-focus hooks to all agent CLIs ([#182](https://github.com/heiervang-technologies/unleash/issues/182)) ([3644c4d](https://github.com/heiervang-technologies/unleash/commit/3644c4dfc788fe2cbbf3d7a0be74fae474ee7e3e))

## [8.4.0](https://github.com/heiervang-technologies/unleash/compare/v8.3.0...v8.4.0) (2026-03-01)


### Features

* squash merge feat/multi-agent-focus-hooks ([e114fe1](https://github.com/heiervang-technologies/unleash/commit/e114fe18fc1c4b2321cbaf0ce23c73851028f4d6))

## [8.3.0](https://github.com/heiervang-technologies/unleash/compare/v8.2.1...v8.3.0) (2026-03-01)


### Features

* remove claude-specific install, fix focus hook opacity logic ([e2c4dbe](https://github.com/heiervang-technologies/unleash/commit/e2c4dbeb90d083e856871d89c4cd745e2a04e267))

## [8.2.1](https://github.com/heiervang-technologies/unleash/compare/v8.2.0...v8.2.1) (2026-03-01)


### Bug Fixes

* remove self-referential symlink creation for unleash ([#178](https://github.com/heiervang-technologies/unleash/issues/178)) ([2554088](https://github.com/heiervang-technologies/unleash/commit/2554088c89f443b19f329558dea06cfadba506b0))

## [8.2.0](https://github.com/heiervang-technologies/unleash/compare/v8.1.2...v8.2.0) (2026-03-01)


### Features

* unify version management into single screen ([#173](https://github.com/heiervang-technologies/unleash/issues/173)) ([96b135f](https://github.com/heiervang-technologies/unleash/commit/96b135fc4c6861e190a03c3469f9ace0318b2bd3))

## [8.1.2](https://github.com/heiervang-technologies/unleash/compare/v8.1.1...v8.1.2) (2026-03-01)


### Bug Fixes

* resolve Cargo.lock merge conflict ([4b2e036](https://github.com/heiervang-technologies/unleash/commit/4b2e036b979817790d9659353affd6d1c948dc08))

## [8.1.1](https://github.com/heiervang-technologies/unleash/compare/v8.1.0...v8.1.1) (2026-03-01)


### Bug Fixes

* remove stale CI job and update docs for unleash rebrand ([#174](https://github.com/heiervang-technologies/unleash/issues/174)) ([080e4fc](https://github.com/heiervang-technologies/unleash/commit/080e4fc426d689ec0d37a572c9a0550f1d8f5b19))

## [8.1.0](https://github.com/heiervang-technologies/agent-unleashed/compare/v8.0.0...v8.1.0) (2026-03-01)


### Features

* **tui:** mouse-friendly TUI with click and scroll support ([#169](https://github.com/heiervang-technologies/agent-unleashed/issues/169)) ([d4d2356](https://github.com/heiervang-technologies/agent-unleashed/commit/d4d2356d738e3a0e6da7c78c4574ae7a222a7660))

## [8.0.0](https://github.com/heiervang-technologies/unleash/compare/v7.5.0...v8.0.0) (2026-02-27)


### ⚠ BREAKING CHANGES

* remove cli.js patching, switch to native-first install ([#161](https://github.com/heiervang-technologies/unleash/issues/161))

### Features

* remove cli.js patching, switch to native-first install ([#161](https://github.com/heiervang-technologies/unleash/issues/161)) ([a03cdf4](https://github.com/heiervang-technologies/unleash/commit/a03cdf4b66e2baf6ccdec7a8d960b25de5c96cbd))
* **tui:** revamp version management with drum picker and 4-agent support ([#165](https://github.com/heiervang-technologies/unleash/issues/165)) ([0cc2246](https://github.com/heiervang-technologies/unleash/commit/0cc224682ea3fab80c1f6dbbb80cbdd35d03a4e8))


### Bug Fixes

* **ci:** update binary name from cu to unleash in release workflows ([#163](https://github.com/heiervang-technologies/unleash/issues/163)) ([0369336](https://github.com/heiervang-technologies/unleash/commit/0369336a91130e952faefac367eb2bd32dbbc9a9))

## [7.5.0](https://github.com/heiervang-technologies/unleash/compare/v7.4.0...v7.5.0) (2026-02-19)


### Features

* **tui:** add keybind hints to profiles screen ([#156](https://github.com/heiervang-technologies/unleash/issues/156)) ([d3d3700](https://github.com/heiervang-technologies/unleash/commit/d3d3700463b3459dbdd032ebd019dbefdff13aa3))

## [7.4.0](https://github.com/heiervang-technologies/unleash/compare/v7.3.0...v7.4.0) (2026-02-18)


### Features

* **ci:** install and enable mold linker in all build jobs ([#154](https://github.com/heiervang-technologies/unleash/issues/154)) ([31fd20a](https://github.com/heiervang-technologies/unleash/commit/31fd20a90e076e00518f6fd7ef41ba38f9174b46))

## [7.3.0](https://github.com/heiervang-technologies/unleash/compare/v7.2.0...v7.3.0) (2026-02-14)


### Features

* add AU_HYPRLAND_FOCUS=1 as default env var for new profiles ([#151](https://github.com/heiervang-technologies/unleash/issues/151)) ([53cbb01](https://github.com/heiervang-technologies/unleash/commit/53cbb012de2850b8e4f633aa2ab8b8b88b5ec183))

## [7.2.0](https://github.com/heiervang-technologies/unleash/compare/v7.1.2...v7.2.0) (2026-02-14)


### Features

* hyprland-focus plugin for window transparency ([#149](https://github.com/heiervang-technologies/unleash/issues/149)) ([8772d3e](https://github.com/heiervang-technologies/unleash/commit/8772d3ef9a18382ccdb779f3db14b02167e362df))

## [7.1.2](https://github.com/heiervang-technologies/unleash/compare/v7.1.1...v7.1.2) (2026-02-13)


### Bug Fixes

* Enter key opens profile editor instead of just selecting ([#147](https://github.com/heiervang-technologies/unleash/issues/147)) ([b62eb05](https://github.com/heiervang-technologies/unleash/commit/b62eb0516f0ac8299847d84df1fe783fb787e73f))

## [7.1.1](https://github.com/heiervang-technologies/unleash/compare/v7.1.0...v7.1.1) (2026-02-13)


### Bug Fixes

* profile edit screen layout for settings visibility ([0908b14](https://github.com/heiervang-technologies/unleash/commit/0908b14f849217badfce861bc68fe808e1882a8a))
* profile edit settings visibility ([9c44b9d](https://github.com/heiervang-technologies/unleash/commit/9c44b9d2eb265863f2f4d753e3fb55ed018cc58b))

## [7.1.0](https://github.com/heiervang-technologies/unleash/compare/v7.0.0...v7.1.0) (2026-02-13)


### Features

* unify settings into profiles ([80d41df](https://github.com/heiervang-technologies/unleash/commit/80d41df88467f89694ba58630f0b721bdd868bd2))
* unify settings into profiles ([cb62cb1](https://github.com/heiervang-technologies/unleash/commit/cb62cb1ff2c78a22f34e80b7a3c09316689b109f))

## [7.0.0](https://github.com/heiervang-technologies/unleash/compare/v6.0.1...v7.0.0) (2026-02-13)


### ⚠ BREAKING CHANGES

* remove c-prefixed backward-compatible entry points (v6) ([#118](https://github.com/heiervang-technologies/unleash/issues/118))
* All backward-compatible entry points prefixed with "c" have been removed. This is a major version bump to v6.0.0.
* Project renamed from claude-unleashed to unleash
* The blacklist system has been replaced with a whitelist system. Only whitelisted versions will be installed when using "latest".
* The `cuw` command has been removed. Use `cu go` or `cug` instead.
* Binary renamed from `cui` to `cu`. The `cui` and `cutx` commands are now symlinks to the main `cu` binary.
* TUI binary renamed from claude-unleashed to cui

### Features

* **v5**: Agent unleashed - agent agnostic wrapper ([#82](https://github.com/heiervang-technologies/unleash/issues/82)) ([dd97f2d](https://github.com/heiervang-technologies/unleash/commit/dd97f2da8b70a04852fa6d9f80ca008b3ea4f976))
* add -d/--daemon flag to cutx for auto-cleanup ([a42f2f5](https://github.com/heiervang-technologies/unleash/commit/a42f2f57a871531283897e2bbe936512173f0b4a))
* add 2.1.4 patch config and improve TUI text input ([02c6c4b](https://github.com/heiervang-technologies/unleash/commit/02c6c4bfb7dc7fbd955662b879ce8380e4d1559b))
* add auto mode patch for Claude Code 2.1.5 ([a37dedf](https://github.com/heiervang-technologies/unleash/commit/a37dedf6789e1139e54d8bc81a1285ae81084301))
* add Claude Code 2.1.29 patch support and fix stop prompt handling ([#113](https://github.com/heiervang-technologies/unleash/issues/113)) ([acd736a](https://github.com/heiervang-technologies/unleash/commit/acd736a46f384e52d0e7ac34260a859a1a16fcb3))
* add Codex version management with whitelist/blacklist filtering ([#117](https://github.com/heiervang-technologies/unleash/issues/117)) ([05d10f8](https://github.com/heiervang-technologies/unleash/commit/05d10f8faa67e21cf02b59d47a36e5e6bd3aa162))
* add comprehensive CLI improvements to cu ([#34](https://github.com/heiervang-technologies/unleash/issues/34)) ([18f3aa6](https://github.com/heiervang-technologies/unleash/commit/18f3aa6a320f28f1f97bf7532a6fb1060f4b8b27))
* Add cutx go command and cutxg shorthand ([1da5a46](https://github.com/heiervang-technologies/unleash/commit/1da5a460074f0fb265af699123192d257010cb32))
* add defensive TTY check before TUI initialization ([06dbe90](https://github.com/heiervang-technologies/unleash/commit/06dbe904fe4ec27a2bc45c1731d42bd082609ea8))
* add headless tmux mode with cutx command ([#17](https://github.com/heiervang-technologies/unleash/issues/17)) ([c37b3e8](https://github.com/heiervang-technologies/unleash/commit/c37b3e8c9c8cf4262bcbfcd3ba99fdfbf828ecc2))
* add Hyprland window manager integration ([#140](https://github.com/heiervang-technologies/unleash/issues/140)) ([e965102](https://github.com/heiervang-technologies/unleash/commit/e9651020a2ea308302c129527555c3fe0bdce099))
* add installation scripts and TUI version management ([83f4121](https://github.com/heiervang-technologies/unleash/commit/83f4121ea9390a52a7b88b1b0e071dcc2f320211))
* add local and pony codex profiles ([be4aa75](https://github.com/heiervang-technologies/unleash/commit/be4aa754d669236c355f479408c62d3b27ee7a42))
* Add MCP Refresh and Process Restart plugins ([#1](https://github.com/heiervang-technologies/unleash/issues/1)) ([b25dfb2](https://github.com/heiervang-technologies/unleash/commit/b25dfb2f7312f513551ea2913175bcbc251712bc))
* Add multi-provider voice output (TTS) plugin with VibeVoice streaming ([a9dc769](https://github.com/heiervang-technologies/unleash/commit/a9dc769701475e82f496bcf64ca3ef5b68091b34))
* add muscular Claude artwork to TUI and installer ([#56](https://github.com/heiervang-technologies/unleash/issues/56)) ([48e2bd4](https://github.com/heiervang-technologies/unleash/commit/48e2bd40c99e8cf9e4df606673733883685d2b5c))
* add native Claude Code installation support alongside npm ([#128](https://github.com/heiervang-technologies/unleash/issues/128)) ([fc27229](https://github.com/heiervang-technologies/unleash/commit/fc27229dc4ae3366643659d7f22ab090d6d4ec7c)), closes [#126](https://github.com/heiervang-technologies/unleash/issues/126)
* add OAuth token authentication handling and timeout configuration ([#16](https://github.com/heiervang-technologies/unleash/issues/16)) ([369af11](https://github.com/heiervang-technologies/unleash/commit/369af11572306a361a0f26233b377a9cd82f888b))
* add OpenAI Codex as submodule ([1fc124b](https://github.com/heiervang-technologies/unleash/commit/1fc124bbf114bd7ea753ef5e54807b0660b9811e))
* add OpenRouter configuration for Codex CLI ([57877cd](https://github.com/heiervang-technologies/unleash/commit/57877cdec8910011fffd1ef3eb94325ac97749e3))
* Add OpenRouter configuration for Codex CLI ([c7ecdb0](https://github.com/heiervang-technologies/unleash/commit/c7ecdb009c1194ee4fd48512087517b9c52c584b))
* add patch config for Claude Code 2.1.12 and stop hook integration tests ([#67](https://github.com/heiervang-technologies/unleash/issues/67)) ([1e769ec](https://github.com/heiervang-technologies/unleash/commit/1e769ec316fcd84e91be5d949fcdcd6618dad28e))
* add private repo support for installation ([a20a92e](https://github.com/heiervang-technologies/unleash/commit/a20a92e9d4a181e79f08ce7f6b1b4e33e06493c8))
* add smooth slide animation for art sidebar ([#65](https://github.com/heiervang-technologies/unleash/issues/65)) ([ea2841d](https://github.com/heiervang-technologies/unleash/commit/ea2841daf79f4629b6834cc0f0a9ee1d3bb88361))
* add supercompact plugin for EITF compaction ([#131](https://github.com/heiervang-technologies/unleash/issues/131)) ([1b0d374](https://github.com/heiervang-technologies/unleash/commit/1b0d374cd0a43d87e3510d4888f0ab87c4eed8d3))
* add supercompact plugin with EITF compaction patch ([370c955](https://github.com/heiervang-technologies/unleash/commit/370c955f754badb85f8b7238f579fdfb815e4190))
* add version blacklist for Claude Code installations ([#26](https://github.com/heiervang-technologies/unleash/issues/26)) ([e791193](https://github.com/heiervang-technologies/unleash/commit/e7911934ad167e018c95a6292e5bbd434355f443))
* auto-merge minor/patch releases, require PR for major ([acf3695](https://github.com/heiervang-technologies/unleash/commit/acf36959cf29899fb34e541172dadf1cebd70659))
* **auto-mode:** add autonomous operation mode plugin ([8c44493](https://github.com/heiervang-technologies/unleash/commit/8c444935f5eb980eec0e1399ba6a06c24f6639eb))
* **auto-mode:** add Stop hook enforcement for auto mode ([556415e](https://github.com/heiervang-technologies/unleash/commit/556415e6ed941cc52dd537dc00d21cc6827d2f97))
* **auto-mode:** add toggle and status line indicator ([4acd10d](https://github.com/heiervang-technologies/unleash/commit/4acd10dc49e9fa318404dae94ca13acd46510289))
* **auto-mode:** improve startup behavior and add env var patches ([ab32ef1](https://github.com/heiervang-technologies/unleash/commit/ab32ef17c88bf873d16dd8509d76453d8ddfddcf))
* **auto-mode:** sync CLI visual via tmux send-keys ([fb7dec3](https://github.com/heiervang-technologies/unleash/commit/fb7dec38f52816737c8827432bf6bb1d10aabb3d))
* change blacklist system to whitelist system ([#53](https://github.com/heiervang-technologies/unleash/issues/53)) ([ad51a07](https://github.com/heiervang-technologies/unleash/commit/ad51a073a69b9ebd52ebcb0c720bc6c437b49a12))
* **ci:** add integration tests for plugins and patches ([7b82e2c](https://github.com/heiervang-technologies/unleash/commit/7b82e2cff02c325593941a334d50591804d61124))
* **client-patch:** auto-patch on Claude version change ([2bad83b](https://github.com/heiervang-technologies/unleash/commit/2bad83bee1e8a580c1575c7ebb5947d849cec8fd))
* **hooks:** add centralized hook management system ([caf2e83](https://github.com/heiervang-technologies/unleash/commit/caf2e83c663580e371b3dec15d8fb89e44c4121c))
* implement fork-based extension system with upstream sync ([661fa64](https://github.com/heiervang-technologies/unleash/commit/661fa64b8460ad3ec2524ce71cf0fcbd0aec646e))
* **live-patch:** add auto mode via client patching ([3314b53](https://github.com/heiervang-technologies/unleash/commit/3314b533715485829ecfaa7d550ac70c11e0d76b))
* **live-patch:** add yellow color for auto mode (Patch 6) ([fd5a5bc](https://github.com/heiervang-technologies/unleash/commit/fd5a5bcadf5b2eb6f42831d4bf680ac5fe6ad0a9))
* make install runs full script, add uninstall command ([1223f37](https://github.com/heiervang-technologies/unleash/commit/1223f37ba3c4ccc25c4f7c613ace8084d6878ac1))
* make TUI optional for headless environments ([d12eb1e](https://github.com/heiervang-technologies/unleash/commit/d12eb1ec267cf36f11c4aab55721b9c5348983c9))
* **patches:** add support for Claude Code v2.1.22 ([a88d38d](https://github.com/heiervang-technologies/unleash/commit/a88d38d5c41eef2bb3d83e8ba4503716449b8250))
* **plugins:** add omnihook plugin for low-latency voice integration ([8eb5dc0](https://github.com/heiervang-technologies/unleash/commit/8eb5dc0137081c32be59147325bc68e9f9cfbc63))
* **process-restart:** add exit-claude command and update docs ([30d000b](https://github.com/heiervang-technologies/unleash/commit/30d000bbab53ff41044098797e779d9475f08bba))
* **process-restart:** add restart-claude command with process isolation ([755712a](https://github.com/heiervang-technologies/unleash/commit/755712af3bb6177ea377652f1e20a466d547be1c))
* **process-restart:** working self-restart with claude-unleashed wrapper ([47b68b1](https://github.com/heiervang-technologies/unleash/commit/47b68b10264bf993dadd85cc44ccb09196447b05))
* remove c-prefixed backward-compatible entry points (v6) ([8d9ec58](https://github.com/heiervang-technologies/unleash/commit/8d9ec584cb4b1f308d642374e6eb4f69a53f5cba))
* remove c-prefixed backward-compatible entry points (v6) ([#118](https://github.com/heiervang-technologies/unleash/issues/118)) ([4300da8](https://github.com/heiervang-technologies/unleash/commit/4300da8a348e20a5ae10e26237709a263eb3a65f))
* remove cuw, use multiple cargo binaries for all commands ([#51](https://github.com/heiervang-technologies/unleash/issues/51)) ([2a3fc24](https://github.com/heiervang-technologies/unleash/commit/2a3fc24cb62275c74e079a9f0ed32b114a34d9c2))
* restructure CLI with subcommands ([0305ba7](https://github.com/heiervang-technologies/unleash/commit/0305ba7d5f7e0b5ac3d0829ffc99462ba39378cf))
* set process arg0 to include wrapper PID for identification ([227dc5c](https://github.com/heiervang-technologies/unleash/commit/227dc5c4b856cff94a8768d561e5e11ee3061b60))
* **tests:** add headless command tests for all subcommands ([#139](https://github.com/heiervang-technologies/unleash/issues/139)) ([8adc8a3](https://github.com/heiervang-technologies/unleash/commit/8adc8a36ee257772cc0c83a222bb3585f60509b1))
* **tui:** add editable entry point in Settings ([632bb51](https://github.com/heiervang-technologies/unleash/commit/632bb5177bbc349077c07990e1c06c63517113c6))
* **tui:** add Ratatui launcher and profile manager ([e203ecd](https://github.com/heiervang-technologies/unleash/commit/e203ecd7a22ed3c6f951215b2487e5279a537bc8))
* **tui:** add Reset Settings option ([d65d956](https://github.com/heiervang-technologies/unleash/commit/d65d956121dd76624baa303cddf8b0dc95732986))
* **tui:** add self-update with git pull and recompile ([a75baea](https://github.com/heiervang-technologies/unleash/commit/a75baea107db3d3ba33b2819e7775a641fa83377))
* **tui:** add viewport scrolling and external editor for stop-prompt ([7393f70](https://github.com/heiervang-technologies/unleash/commit/7393f70785ab9386945edd964591e0099907d127))
* unify cu, cui, cutx into single binary ([3d23a33](https://github.com/heiervang-technologies/unleash/commit/3d23a338d9683469969c5be81be25061391ed723))
* Update README with animation demo GIF ([73c6126](https://github.com/heiervang-technologies/unleash/commit/73c61260ecffe5173f82e5f7ad7a85bbd8c7891a))
* use self-hosted runners for all GitHub Actions workflows ([#47](https://github.com/heiervang-technologies/unleash/issues/47)) ([65ad326](https://github.com/heiervang-technologies/unleash/commit/65ad326c3fe6afbd05532fa087a896cebf011153))
* whitelist Claude Code v2.1.32 and add whitelisting requirements doc ([#119](https://github.com/heiervang-technologies/unleash/issues/119)) ([a27dfdf](https://github.com/heiervang-technologies/unleash/commit/a27dfdf96492ce1d6f514a405cda842137f6c043))
* whitelist Claude Code v2.1.37 ([#124](https://github.com/heiervang-technologies/unleash/issues/124)) ([c7293ab](https://github.com/heiervang-technologies/unleash/commit/c7293ab83f59bb38391524c5145c31d704cef14b))
* whitelist Codex 0.98.0 ([3a8cd66](https://github.com/heiervang-technologies/unleash/commit/3a8cd667d5afc83147e6fc82a8c73dc9892e5e08))


### Bug Fixes

* add --force flag to npm install for version downgrades ([#60](https://github.com/heiervang-technologies/unleash/issues/60)) ([e30a1f5](https://github.com/heiervang-technologies/unleash/commit/e30a1f53eedf71ea1bf43fda007e5bc5e4d91aab))
* add --version support to mock claude in CI combined test ([641b349](https://github.com/heiervang-technologies/unleash/commit/641b349e445359b5e433ac39d951f5d9dc634e62))
* add auto-onboard bypass and whitelist version 2.1.14 ([a53e720](https://github.com/heiervang-technologies/unleash/commit/a53e720d7ba41fed9c4284e6711ede81762d796c)), closes [#83](https://github.com/heiervang-technologies/unleash/issues/83)
* add name field to restart command frontmatter ([bbe8d5f](https://github.com/heiervang-technologies/unleash/commit/bbe8d5fbd2acb6666044b3f29b845f35e8b49b6b))
* add trailing newline before reversing version list ([27613a1](https://github.com/heiervang-technologies/unleash/commit/27613a111d8621af72623651a051df30bc5444f3))
* address additional critical issues in cutx ([cc8f47a](https://github.com/heiervang-technologies/unleash/commit/cc8f47a2f2ec91ff97228d4edacc864dd9c68e79))
* address code review issues ([#61](https://github.com/heiervang-technologies/unleash/issues/61)) ([0a27c3b](https://github.com/heiervang-technologies/unleash/commit/0a27c3b47a45f1a833f710b33084cf8d6e439fff))
* address critical issues in cutx headless mode ([946d1b5](https://github.com/heiervang-technologies/unleash/commit/946d1b54afb4a083dd6bba405615d4f3c698392b))
* align binary artifact names with release workflow ([41556c7](https://github.com/heiervang-technologies/unleash/commit/41556c7a9ed030677c2bba017332f8cabd927e7f))
* allow Escape to quit/exit in addition to q ([1b0b5a7](https://github.com/heiervang-technologies/unleash/commit/1b0b5a71e903c3cf31519c9c25e018b2a810053f))
* Async version loading and settings text truncation ([6bed2df](https://github.com/heiervang-technologies/unleash/commit/6bed2df607feeab6a8095d7273c7ba0e3840c9eb))
* auto-configure .claude.json to skip onboarding and bypass warnings ([0771ba0](https://github.com/heiervang-technologies/unleash/commit/0771ba0ce6955fda7f985318d6e14a0c29617044))
* auto-merge should run whenever PR exists ([9f919be](https://github.com/heiervang-technologies/unleash/commit/9f919be73ffbdc56eb7ba4bc9e4cb22c9957d0b4))
* **auto-mode:** add ctrl+tab hint and shorten Stop hook message ([3605b33](https://github.com/heiervang-technologies/unleash/commit/3605b33352e309f8e30f9d98f036365109d11a6b))
* **auto-mode:** add missing hooks.json for stop hook ([0e953fa](https://github.com/heiervang-technologies/unleash/commit/0e953fafb575069bfddb609f2ba84fd7459da514))
* **auto-mode:** clarify Stop hook guidance for better autonomy ([bd7d9a7](https://github.com/heiervang-technologies/unleash/commit/bd7d9a747d4272bf6bd952dbae9b32c7bae9ea85))
* **auto-mode:** clarify that Claude can run exit-claude itself ([288c224](https://github.com/heiervang-technologies/unleash/commit/288c224bee7a179defea7944a261a3a2414a8ccc))
* **auto-mode:** make flag files wrapper-specific for session isolation ([91af680](https://github.com/heiervang-technologies/unleash/commit/91af680ab7ffffbb9e602aedbc66968185df3e5c))
* **auto-mode:** remove non-functional ctrl+tab hint from Stop hook ([dc05f9a](https://github.com/heiervang-technologies/unleash/commit/dc05f9a8128244d19b8a171a0a392074a3a750ee))
* **auto-mode:** use CLAUDE_UNLEASHED_ROOT env var for script path ([df5c00b](https://github.com/heiervang-technologies/unleash/commit/df5c00bc348fa546dd6f0c77d693c7e1e14ae60f))
* cache installed version to avoid subprocess on every TUI frame ([#25](https://github.com/heiervang-technologies/unleash/issues/25)) ([f403428](https://github.com/heiervang-technologies/unleash/commit/f40342831d25fb8cf74b0b71d6ddc67ddc369daf))
* **ci:** install clang/mold for Rust builds and rewrite upstream sync ([7b0582f](https://github.com/heiervang-technologies/unleash/commit/7b0582f95a5011cbefdce4b5517f45af063c3bf9))
* **client-patch:** add support for Claude v2.1.2 patterns ([818fe15](https://github.com/heiervang-technologies/unleash/commit/818fe1541c30dc80699aed8436b48142f646bb2b))
* complete shellcheck SC2155 and SC2034 fixes ([a47e27c](https://github.com/heiervang-technologies/unleash/commit/a47e27ce5d7e87e34033b603a17a6e04c67b9dab))
* correct Rust edition from 2024 to 2021 ([6fb79d9](https://github.com/heiervang-technologies/unleash/commit/6fb79d97afbb10092738f6332c5df700b60ccfe9))
* correct TUI release workflow paths and add multi-platform builds ([64820e7](https://github.com/heiervang-technologies/unleash/commit/64820e79703208f36e310cf4a9a48fb5f38eae60))
* Esc is Back, Back on main menu quits ([0a56eb5](https://github.com/heiervang-technologies/unleash/commit/0a56eb5065cca77e34c53ae05a3e17fb9a409523))
* explicitly pass secrets to spawn-agent workflow ([#5](https://github.com/heiervang-technologies/unleash/issues/5)) ([efe7f4d](https://github.com/heiervang-technologies/unleash/commit/efe7f4d976f981cf43f17cf50f87e93a295c400c))
* handle multiline TOML stop_prompt in auto-mode hook ([#115](https://github.com/heiervang-technologies/unleash/issues/115)) ([8ac908b](https://github.com/heiervang-technologies/unleash/commit/8ac908ba0f7f11fee845b11bffccb7bad7d39745))
* hide exit code 143 when Claude is terminated via SIGTERM ([#9](https://github.com/heiervang-technologies/unleash/issues/9)) ([fb4b296](https://github.com/heiervang-technologies/unleash/commit/fb4b296b2b7703138efda875a764954a40370a91))
* **hooks:** don't sync plugin hooks to settings.json ([fa80ba6](https://github.com/heiervang-technologies/unleash/commit/fa80ba6eab2a04e3736a8e3e0c852949bc6aaa5e))
* improve README structure and installer update logic ([16494c6](https://github.com/heiervang-technologies/unleash/commit/16494c60dc47ae4e4844ec4ad2b25e07b28b5a79))
* install plugins globally for /auto command to work everywhere ([#35](https://github.com/heiervang-technologies/unleash/issues/35)) ([275a53e](https://github.com/heiervang-technologies/unleash/commit/275a53e26bff45b85f60672e593456b76c90f111))
* **live-patch:** comprehensive permission bypass and »» icon ([359ec09](https://github.com/heiervang-technologies/unleash/commit/359ec092192a76f58201173702080355218800fd))
* **live-patch:** use l9 fs module instead of require for ESM compatibility ([fb35f11](https://github.com/heiervang-technologies/unleash/commit/fb35f11b67ec4806d4f15ddadf65d4d3998cd101))
* load plugins via wrapper and update defaults ([4ac76b8](https://github.com/heiervang-technologies/unleash/commit/4ac76b8a83cd288054f28f91e258c1b8e3c577b7))
* move Codex submodule to codex-unleashed/ directory ([b6f5ee1](https://github.com/heiervang-technologies/unleash/commit/b6f5ee1c5448b2f4fbc2a5d1d486348b5e21eddb))
* organize Codex integration in codex-unleashed/ directory ([8a26fcb](https://github.com/heiervang-technologies/unleash/commit/8a26fcb00f55167847ebb2bcce318fa934d1c76c))
* patch-claude.sh headless mode and 2.1.3 support ([0865b57](https://github.com/heiervang-technologies/unleash/commit/0865b5779a4450dd24ef2bda7bc99a578a6078c4))
* patch-claude.sh support for 2.1.3 full modes array ([246e6cc](https://github.com/heiervang-technologies/unleash/commit/246e6cc31aec5ba2543bdc5e939cf7db75d873c7))
* **patches:** add auto mode to validation function ([dc43e0d](https://github.com/heiervang-technologies/unleash/commit/dc43e0dee998726b5f5c9a4a1d85ef3a499d7b32))
* **plugins:** deduplicate plugin directories ([86cd7b8](https://github.com/heiervang-technologies/unleash/commit/86cd7b82f6df85f5edaf4399ec1f4f17c80c6ada))
* point claude symlink to npm cli.js instead of managed binary ([#111](https://github.com/heiervang-technologies/unleash/issues/111)) ([e1b06f2](https://github.com/heiervang-technologies/unleash/commit/e1b06f23462797579eb690399d21a34282675373))
* pre-populate stop prompt field with default message ([84c8d1d](https://github.com/heiervang-technologies/unleash/commit/84c8d1de743d40dc16928267afa5b896968fbda8))
* prevent TUI visual corruption during Codex build ([#142](https://github.com/heiervang-technologies/unleash/issues/142)) ([0f77aca](https://github.com/heiervang-technologies/unleash/commit/0f77aca44786b5047371d9d65f2b2f489c2b3f79)), closes [#100](https://github.com/heiervang-technologies/unleash/issues/100)
* read default stop prompt from hook script (source of truth) ([60c1a82](https://github.com/heiervang-technologies/unleash/commit/60c1a8298c93a5b9d4a4b31c685d5deadf4d64a2))
* remove 2.1.14 from whitelist ([0f4c2eb](https://github.com/heiervang-technologies/unleash/commit/0f4c2eb8dee77b29f67c1a111df0f71080df22df))
* remove unused TARGET variable in install script ([a59324d](https://github.com/heiervang-technologies/unleash/commit/a59324dd152e78c1fa8444d23675a1e9b6aaa969))
* rename package to claude-unleashed, use edition 2024 ([91f740b](https://github.com/heiervang-technologies/unleash/commit/91f740b18631f11f261cc7d4680ab9b6937fd855))
* rename supabase to postgrest in workflow inputs ([c34d62e](https://github.com/heiervang-technologies/unleash/commit/c34d62e34a14729161061f86b8e285b619de1a4d))
* rename supabase to postgrest in workflow inputs ([1889f93](https://github.com/heiervang-technologies/unleash/commit/1889f93befd276a8918867151ea2d805fcbe23bc))
* resolve all CI test failures (shellcheck + patch tests) ([68166b6](https://github.com/heiervang-technologies/unleash/commit/68166b62ac991e8200ec9573010b72d9e4b804d9))
* resolve all remaining shellcheck warnings ([96e0370](https://github.com/heiervang-technologies/unleash/commit/96e0370d49c8825c954093b5ea6bc48dd8d50f00))
* resolve all shellcheck warnings in test files ([6a062d3](https://github.com/heiervang-technologies/unleash/commit/6a062d370c9ed2edcfcf94fd38eaae1fb6305cb1))
* resolve compiler warning for shared main.rs across binaries ([#138](https://github.com/heiervang-technologies/unleash/issues/138)) ([7159939](https://github.com/heiervang-technologies/unleash/commit/715993937d1657c665713f89791dda4fc8ca8dae)), closes [#39](https://github.com/heiervang-technologies/unleash/issues/39)
* resolve final 2 shellcheck warnings ([8d00564](https://github.com/heiervang-technologies/unleash/commit/8d00564c456ad9e75f02ac2d6a9bc24b48c1d0c7))
* restore comprehensive default stop prompt for auto-mode ([c437ee1](https://github.com/heiervang-technologies/unleash/commit/c437ee1d7adb26a74c9f70ee7cd6fe0d3bb62f6a))
* support GH_PAT and GITHUB_TOKEN in addition to GH_TOKEN ([4f4dfde](https://github.com/heiervang-technologies/unleash/commit/4f4dfde99b3a4986dd7a5c08ab8e82828e374cb0))
* suppress 404 errors when binary not available ([39e9d2f](https://github.com/heiervang-technologies/unleash/commit/39e9d2f73886e63c1ffa610931275c9f3266b562))
* update claude-code submodule to use Anthropic's official repository ([#13](https://github.com/heiervang-technologies/unleash/issues/13)) ([eceeacf](https://github.com/heiervang-technologies/unleash/commit/eceeacfc1383357b0c5e4b06f016fdf3fd2fa78f))
* update plugin manifests to correct Claude Code schema ([b366be6](https://github.com/heiervang-technologies/unleash/commit/b366be61f2376715960c77a5570bf9a689fc1618))
* update TUI update logic, documentation paths, and install scripts ([b88220e](https://github.com/heiervang-technologies/unleash/commit/b88220e4a09abb15c3f6e11591fd2e665c453c36))
* update TUI update logic, documentation paths, and install scripts ([7a1f78c](https://github.com/heiervang-technologies/unleash/commit/7a1f78c7a8a4a350ae4d9d8a247474c098ec993a))
* use gh cli for binary downloads from private repos ([8a649cb](https://github.com/heiervang-technologies/unleash/commit/8a649cb6d745a4d089beb17fee9cbcc88afa6930))
* use reusable mention-trigger workflow from core ([#32](https://github.com/heiervang-technologies/unleash/issues/32)) ([2457e50](https://github.com/heiervang-technologies/unleash/commit/2457e50966152244b211da74d71d5e978189bcb1))
* use Zot registry instead of Docker Hub for snail image ([#75](https://github.com/heiervang-technologies/unleash/issues/75)) ([23ca208](https://github.com/heiervang-technologies/unleash/commit/23ca208a348a14ab90672ef7735c714d7702ba64))
* **voice-output:** use venv Python in hook handler ([57c7a2d](https://github.com/heiervang-technologies/unleash/commit/57c7a2d54d88eeb0605ec8057c115a96fbc6b903))
* **wrapper:** add --dangerously-skip-permissions flag ([7f9b463](https://github.com/heiervang-technologies/unleash/commit/7f9b46389ad2e2f974d7c488cc7ea141210ad906))
* **wrapper:** resolve symlinks to find plugins ([155dfbd](https://github.com/heiervang-technologies/unleash/commit/155dfbdfb021e09137ea3676e0ed0a74394917bb))


### Code Refactoring

* unify CLI entry points as cu/cui/cutx ([#19](https://github.com/heiervang-technologies/unleash/issues/19)) ([e3728ce](https://github.com/heiervang-technologies/unleash/commit/e3728ce5bb245efde5f810042a3b28d1db598ccd))

## [6.0.1](https://github.com/heiervang-technologies/unleash/compare/v6.0.0...v6.0.1) (2026-02-12)


### Bug Fixes

* resolve compiler warning for shared main.rs across binaries ([#138](https://github.com/heiervang-technologies/unleash/issues/138)) ([7159939](https://github.com/heiervang-technologies/unleash/commit/715993937d1657c665713f89791dda4fc8ca8dae)), closes [#39](https://github.com/heiervang-technologies/unleash/issues/39)

## [6.0.0](https://github.com/heiervang-technologies/unleash/compare/v5.8.0...v6.0.0) (2026-02-12)


### ⚠ BREAKING CHANGES

* remove c-prefixed backward-compatible entry points (v6) ([#118](https://github.com/heiervang-technologies/unleash/issues/118))
* All backward-compatible entry points prefixed with "c" have been removed. This is a major version bump to v6.0.0.

### Features

* remove c-prefixed backward-compatible entry points (v6) ([8d9ec58](https://github.com/heiervang-technologies/unleash/commit/8d9ec584cb4b1f308d642374e6eb4f69a53f5cba))
* remove c-prefixed backward-compatible entry points (v6) ([#118](https://github.com/heiervang-technologies/unleash/issues/118)) ([4300da8](https://github.com/heiervang-technologies/unleash/commit/4300da8a348e20a5ae10e26237709a263eb3a65f))

## [5.8.0](https://github.com/heiervang-technologies/unleash/compare/v5.7.0...v5.8.0) (2026-02-11)


### Features

* whitelist Codex 0.98.0 ([3a8cd66](https://github.com/heiervang-technologies/unleash/commit/3a8cd667d5afc83147e6fc82a8c73dc9892e5e08))

## [5.7.0](https://github.com/heiervang-technologies/unleash/compare/v5.6.0...v5.7.0) (2026-02-11)


### Features

* add supercompact plugin with EITF compaction patch ([370c955](https://github.com/heiervang-technologies/unleash/commit/370c955f754badb85f8b7238f579fdfb815e4190))

## [5.6.0](https://github.com/heiervang-technologies/unleash/compare/v5.5.0...v5.6.0) (2026-02-09)


### Features

* add native Claude Code installation support alongside npm ([#128](https://github.com/heiervang-technologies/unleash/issues/128)) ([fc27229](https://github.com/heiervang-technologies/unleash/commit/fc27229dc4ae3366643659d7f22ab090d6d4ec7c)), closes [#126](https://github.com/heiervang-technologies/unleash/issues/126)

## [5.5.0](https://github.com/heiervang-technologies/unleash/compare/v5.4.0...v5.5.0) (2026-02-09)


### Features

* whitelist Claude Code v2.1.37 ([#124](https://github.com/heiervang-technologies/unleash/issues/124)) ([c7293ab](https://github.com/heiervang-technologies/unleash/commit/c7293ab83f59bb38391524c5145c31d704cef14b))

## [5.4.0](https://github.com/heiervang-technologies/unleash/compare/v5.3.0...v5.4.0) (2026-02-08)


### Features

* add local and pony codex profiles ([be4aa75](https://github.com/heiervang-technologies/unleash/commit/be4aa754d669236c355f479408c62d3b27ee7a42))

## [5.3.0](https://github.com/heiervang-technologies/unleash/compare/v5.2.0...v5.3.0) (2026-02-06)


### Features

* add Codex version management with whitelist/blacklist filtering ([#117](https://github.com/heiervang-technologies/unleash/issues/117)) ([05d10f8](https://github.com/heiervang-technologies/unleash/commit/05d10f8faa67e21cf02b59d47a36e5e6bd3aa162))

## [5.2.0](https://github.com/heiervang-technologies/unleash/compare/v5.1.1...v5.2.0) (2026-02-06)


### Features

* whitelist Claude Code v2.1.32 and add whitelisting requirements doc ([#119](https://github.com/heiervang-technologies/unleash/issues/119)) ([a27dfdf](https://github.com/heiervang-technologies/unleash/commit/a27dfdf96492ce1d6f514a405cda842137f6c043))

## [5.1.1](https://github.com/heiervang-technologies/unleash/compare/v5.1.0...v5.1.1) (2026-02-02)


### Bug Fixes

* handle multiline TOML stop_prompt in auto-mode hook ([#115](https://github.com/heiervang-technologies/unleash/issues/115)) ([8ac908b](https://github.com/heiervang-technologies/unleash/commit/8ac908ba0f7f11fee845b11bffccb7bad7d39745))

## [5.1.0](https://github.com/heiervang-technologies/unleash/compare/v5.0.2...v5.1.0) (2026-02-02)


### Features

* add Claude Code 2.1.29 patch support and fix stop prompt handling ([#113](https://github.com/heiervang-technologies/unleash/issues/113)) ([acd736a](https://github.com/heiervang-technologies/unleash/commit/acd736a46f384e52d0e7ac34260a859a1a16fcb3))

## [5.0.2](https://github.com/heiervang-technologies/unleash/compare/v5.0.1...v5.0.2) (2026-02-02)


### Bug Fixes

* point claude symlink to npm cli.js instead of managed binary ([#111](https://github.com/heiervang-technologies/unleash/issues/111)) ([e1b06f2](https://github.com/heiervang-technologies/unleash/commit/e1b06f23462797579eb690399d21a34282675373))

## [5.0.1](https://github.com/heiervang-technologies/unleash/compare/v5.0.0...v5.0.1) (2026-01-29)


### Bug Fixes

* **ci:** install clang/mold for Rust builds and rewrite upstream sync ([7b0582f](https://github.com/heiervang-technologies/unleash/commit/7b0582f95a5011cbefdce4b5517f45af063c3bf9))

## [5.0.0](https://github.com/heiervang-technologies/claude-unleashed/compare/v4.10.0...v5.0.0) (2026-01-29)


### ⚠ BREAKING CHANGES

* Project renamed from claude-unleashed to unleash

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
