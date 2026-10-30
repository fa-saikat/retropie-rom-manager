# Contributing

Thanks for helping out. Bug reports, detection fixes and pull requests are all welcome.

## Reporting issues

Use one of the [issue forms](https://github.com/fa-saikat/retropie-rom-manager/issues/new/choose). They ask for the version, distribution, hardware and display server up front, which is usually what it takes to reproduce a problem. For anything that looks like a rendering or startup problem, run the app as `RUST_LOG=info retropie-rom-manager` and include the output.

Never attach or link ROM files. The file name and size are enough for detection bugs.

Security problems go through [private reporting](SECURITY.md), not public issues.

## Development setup

You'll need a recent stable Rust toolchain and the headers GPUI links against. On Debian or Ubuntu:

```sh
sudo apt install libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
  libx11-xcb-dev libxcb1-dev libfontconfig-dev libfreetype-dev libvulkan-dev libzstd-dev
```

Then:

```sh
cargo run            # debug build
cargo test           # detection, grouping, scraping and launching
./scripts/build-deb.sh   # .deb in target/debian/, needs cargo-deb
```

If rustc crashes with a stack overflow while compiling `gpui-component`, set `RUST_MIN_STACK=16777216`. CI sets it too.

Debug builds read a few `ROM_MANAGER_*` environment variables that open the app straight into a given system, dialog or view. They're listed at the end of the README's "Building from source" section.

`vendor/gpui-pre-wgpu` is a patched copy of a GPUI crate, for [zed#65765](https://github.com/zed-industries/zed/issues/65765). Leave it alone unless you're working on that fix, and expect it to go away once upstream has it.

## Pull requests

- Branch from `main` and open the pull request against `main`. Keep each one to a single change.
- Write commit subjects in the imperative, the way the history already reads: "Fix blank window on Mali GPUs", not "fixed" or "fixes".
- Run `cargo test`, and try the change in the app if it's something you can see.
- Update the README if you change behavior, requirements or known limitations.
- Don't bump the version or edit `packaging/changelog`. That happens in a release PR.

CI builds the amd64 and arm64 `.deb` packages for every pull request. You can download them from the run's artifacts to test on real hardware.

## Versioning

Releases follow [Semantic Versioning](https://semver.org): `MAJOR.MINOR.PATCH`. For an app, "the API" means what users rely on, so versions bump as follows:

| Bump | When | Examples |
| --- | --- | --- |
| **Major** | Something users rely on is removed, or stops working the way it did | Dropping an architecture or a supported distribution, raising the minimum glibc, removing a feature, changing what the app writes to the ROM folder or RetroPie's config |
| **Minor** | Something new, compatible with what's there | A new feature, a newly supported system, a new architecture |
| **Patch** | Fixes, with nothing new to learn | Bug and detection fixes, performance work, dependency updates, packaging-only fixes |

Some rules that keep the version history clean:

- **Versions only go up.** CI refuses to release a version lower than the latest tag.
- **Every release gets a new version, including packaging-only fixes.** The Debian revision always stays `-1`, so a fixed `2.1.0` package ships as `2.1.1-1`, not `2.1.0-2`.
- **Tags are created by CI and never moved or reused.** A release is the tag `vX.Y.Z` on main, built and published by the release workflow. Don't tag by hand.
- **There are no pre-release versions.** Builds for testing come from pull request artifacts.

## Releasing

Maintainers only.

1. Branch `release/X.Y.Z` from an up-to-date `main`.
2. Set `version` in `Cargo.toml`, and run `cargo check` so `Cargo.lock` picks it up.
3. Run `./scripts/generate-changelog.sh`. It fetches tags and prepends an entry built from commit subjects since the last release.
4. Rewrite that entry for users: what changed for them, not how. These bullets become the GitHub release notes.
5. Commit as `release: bump to X.Y.Z` and open a pull request titled `release: X.Y.Z`.
6. Merge once CI is green. The release workflow tags `vX.Y.Z` and publishes both `.deb` files with `SHA256SUMS`.

## Code of conduct

Everyone taking part is expected to follow the [code of conduct](CODE_OF_CONDUCT.md).
