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
cargo run      # debug build
cargo test     # detection, grouping, scraping and launching
cargo clippy --all-targets -- -D warnings
cargo deb      # .deb in target/debian/, needs `cargo install cargo-deb`
```

If rustc crashes with a stack overflow while compiling `gpui-component`, set `RUST_MIN_STACK=16777216`. CI sets it too.

Debug builds read a few `ROM_MANAGER_*` environment variables that open the app straight into a given system, dialog or view. They're listed at the end of the README's "Building from source" section.

`vendor/gpui-pre-wgpu` is a patched copy of a GPUI crate, for [zed#65765](https://github.com/zed-industries/zed/issues/65765). Leave it alone unless you're working on that fix, and expect it to go away once upstream has it.

## Pull requests

- Branch from `main` and open the pull request against `main`. Keep each one to a single change.
- Write commit subjects in the imperative, the way the history already reads: "Fix blank window on Mali GPUs", not "fixed" or "fixes".
- Run `cargo test` and `cargo clippy --all-targets -- -D warnings`, and try the change in the app if it's something you can see.
- Update the README if you change behavior, requirements or known limitations.
- Don't bump the version or edit `packaging/changelog`. That happens in a release PR.

CI runs clippy and the tests on every pull request. It doesn't build packages for them. Those are built only for a release, so test your change locally before you push it. To build test packages for real hardware, a maintainer can start the CI workflow by hand from the Actions tab. That builds and test-installs both `.deb` files and keeps them as run artifacts for 3 days.

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
- **Tags come from publishing a release and are never moved or reused.** A release is the tag `vX.Y.Z` on main. CI drafts it, and publishing the draft creates the tag. Don't tag by hand.
- **There are no pre-release versions.** Test builds come from a manual run of the CI workflow.

## Releasing

Maintainers only.

1. Branch `release/X.Y.Z` from an up-to-date `main`.
2. Set `version` in `Cargo.toml`, and run `cargo check` so `Cargo.lock` picks it up.
3. Add an entry at the top of `packaging/changelog`, in the same format as the ones below it. Write it for users: what changed for them, not how. The date line comes from `date -R`.

   ```
   retropie-rom-manager (X.Y.Z-1) unstable; urgency=low

     * What changed, in a sentence.

    -- Your Name <you@example.com>  Fri, 30 Oct 2026 12:00:00 +0600
   ```

4. Commit as `release: bump to X.Y.Z` and open a pull request titled `release: X.Y.Z`. CI checks that the version goes up and that the changelog entry matches it.
5. Merge once CI is green. CI then builds both `.deb` files and installs and removes them on Debian 12, Debian 13, Ubuntu 22.04 and Ubuntu 24.04 to check that their dependencies resolve. If they pass, it creates a draft release `vX.Y.Z` with them, `SHA256SUMS`, the changelog bullets and install commands.
6. Check the draft on the Releases page, edit the notes if you like, and publish it. Publishing creates the `vX.Y.Z` tag.

## Code of conduct

Everyone taking part is expected to follow the [code of conduct](CODE_OF_CONDUCT.md).
