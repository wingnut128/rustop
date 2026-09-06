# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.2](https://github.com/wingnut128/rustop/compare/v0.2.1...v0.2.2) - 2026-07-28

### Security

- bump anyhow (RUSTSEC-2026-0190), tighten release.yml permissions, add Rust CodeQL ([#14](https://github.com/wingnut128/rustop/pull/14))

## [0.2.1](https://github.com/wingnut128/rustop/compare/v0.2.0...v0.2.1) - 2026-07-28

### Other

- Add build provenance attestation and release-time SBOM ([#12](https://github.com/wingnut128/rustop/pull/12))

## [0.2.0](https://github.com/wingnut128/rustop/compare/v0.1.0...v0.2.0) - 2026-07-28

### Added

- Release automation via release-plz: a standing Release PR now bumps the version and regenerates this changelog, and merging it tags + drafts a GitHub Release for the tag-triggered build workflow to attach binaries to

### Changed

- Improved terminal usability
- Bumped `sysinfo` and `ratatui` dependencies

### Security

- Hardened CI: SHA-pinned all GitHub Actions, added a pin-check workflow, and applied branch protection, secret scanning, and fork-PR approval gating to the repo
