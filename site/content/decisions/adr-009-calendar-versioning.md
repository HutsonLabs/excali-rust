+++
title = "ADR-009: Calendar versioning"
description = "Releases are numbered YY.M.BUILD; the first is 26.9.1. GitHub releases only for now, crates.io later."
weight = 9
+++

**Status.** Accepted, owner decision of 2026-09-27 (recorded by `ex-008`).

## Question

How are releases of the port numbered and where are they published? The plan said "tag 1.0" and "publish crates to crates.io" without a scheme.

## Evidence

- The owner's decision (2026-09-27): versioning is calendar `YY.M.BUILD`, the first release is `26.9.1` with tag `v26.9.1`; agents may create git tags and GitHub releases with the ES module + WASM tarball; nothing is published to crates.io, npm or any other registry for now (`ex-801` stays held); 1.0 (26.9.1) does not wait on crates.io.
- calver.org (read 2026-09-28) names the conventions: `YY` "Short year - 6, 16, 106" and `MM` "Short month - 1, 2 ... 11, 12", as distinct from the zero-padded `0Y`/`0M`; pip uses `YY.MINOR.MICRO`.
- Semantic Versioning 2.0.0, item 2 (`semver/semver` `semver.md:62-63`, read 2026-09-28): "A normal version number MUST take the form X.Y.Z where X, Y, and Z are non-negative integers, and MUST NOT contain leading zeroes." So `26.9.1` is a valid SemVer string and `26.09.1` is not.
- The Cargo manifest reference (doc.rust-lang.org/cargo/reference/manifest.html, read 2026-09-28): "The version field is formatted according to the SemVer specification". Every crate in this workspace takes `version.workspace = true` from `[workspace.package]` in `Cargo.toml`.

## Decision

1. The version is `YY.M.BUILD`: two-digit year, month without a leading zero, and a build number that starts at 1 in each month and increments per release within it. No pre-release or build-metadata suffixes. The first release is **26.9.1**.
2. The tag is the version prefixed with `v` (`v26.9.1`). A release is a git tag plus a GitHub release (`gh release create v26.9.1 ...`) carrying the ES module + WASM tarball and its SHA-256 (`ex-802`, `ex-804`). Agents create both.
3. One number for the whole workspace: `[workspace.package] version` in `Cargo.toml`, inherited by every crate. `scripts/gates/version.py check` enforces the format, the inheritance and agreement with `Cargo.lock` in CI; `version.py tag` prints the tag and `version.py next [YYYY-MM]` the next version.
4. Registries are held. Nothing goes to crates.io, npm or any other registry until the owner lifts the hold on `ex-801`. The crates keep `publish = false` until then.

## Consequences

- The workspace moves from `0.1.0` to `26.9.1` now, so the version in every built artefact is the one the release will carry.
- When crates.io is enabled, Cargo reads `26.9.1` as SemVer: a caret requirement `^26.9.1` accepts later `26.x.y` releases and refuses `27.1.1`. Each new year is therefore a SemVer-major step and months within a year are minor steps; a breaking change inside a year needs a note in the release and, if it matters to downstream crates, waiting for the year boundary or an ADR.
- The site's "1.0" language means the first calendar release, `26.9.1`.

## What would reverse it

An owner decision to publish to a registry whose consumers require SemVer-meaningful majors, or a downstream integrator that pins by caret and is broken within a year.
