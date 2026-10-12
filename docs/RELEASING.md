# Releasing writemorebetter

Releases are cut by the repository owner (`cmlarsen`) from `main` with the **Release**
workflow. There is no tag-push trigger on purpose: `release.json` has to sit inside the
tagged commit, so the commit cannot already exist when the tag is created. The workflow
does the commit, the tag and the GitHub release itself.

Tags are plain `X.Y.Z` (no leading `v`), matching the tag style SwiftPM needs for the
binary target URL.

## What a release publishes

| Asset | Built from |
|---|---|
| `writemorebetter-data-<version>.zip` | `scripts/build-data.sh` on ubuntu-latest — the two LGPL-2.1 data binaries (`en_tokenizer.bin`, `en_rules.bin`) with `SHA256SUMS`, `README.md` and `LICENSE-LGPL-2.1.txt` |
| `WriteMoreBetterFFI.xcframework.zip` | `scripts/build-xcframework.sh` on macos-15 — the SwiftPM binary target |
| `SHA256SUMS` | sha256 of both zip archives |

Both jobs use the same pinned Rust toolchain (`1.99.0`), which is an input to the
reproducible artifacts. The data binaries are **not** inside the XCFramework zip: the
checker loads them from disk, so an app ships them as resources.

## Before you start

1. `CHANGELOG.md` must have a `# <version>` section — the release notes are copied from
   it verbatim, and the run fails if the section is missing.
2. Every workspace crate that ships together — `nlprule`, `build`, `swift`
   (`writemorebetter-swift`) and `wasm` — must already be at `<version>` on `main`.
   (`srx` and `python` are versioned independently.) The run fails on a mismatch.
3. The tag `<version>` must not exist yet. The run fails if it does.

## Run it

Dispatch the workflow from `main`, dry run first. From the command line:

```sh
gh workflow run release.yml --repo cmlarsen/writemorebetter --ref main \
  -f version=0.7.0 -f dry_run=true
gh run watch --repo cmlarsen/writemorebetter --exit-status
```

The dry run builds everything and uploads the two zip archives as workflow artifacts.
It **never** commits, tags or publishes. Check, in the run log:

- the artifact list (`writemorebetter-data-<version>`, `writemorebetter-xcframework-<version>`),
- the **Consumer check** job, which resolves the package through `release.json` and runs a
  tiny app that loads the data and calls `check`.

Then cut the real release by dispatching again with `dry_run=false`:

```sh
gh workflow run release.yml --repo cmlarsen/writemorebetter --ref main \
  -f version=0.7.0 -f dry_run=false
```

## What the release run commits and tags

The `publish` job, only when `dry_run` is false:

1. writes `release.json` with the release asset URL
   (`…/releases/download/<version>/WriteMoreBetterFFI.xcframework.zip`) and the SwiftPM
   checksum (`swift package compute-checksum` of the zip),
2. commits it to `main` as `writemorebetter release bot <release-bot@users.noreply.github.com>`,
3. tags that commit `<version>` and pushes the branch and the tag,
4. creates the GitHub release on the tag with both zips, `SHA256SUMS` and the changelog
   section as the notes.

So a release is exactly one commit on `main` that flips `release.json` to the new asset,
plus the tag and the release pointing at it.

## Verify the published checksum

The app's binary target is verified by SwiftPM against `release.json`, but you can check
the published asset by hand:

```sh
gh release download 0.7.0 --repo cmlarsen/writemorebetter -p WriteMoreBetterFFI.xcframework.zip
swift package compute-checksum WriteMoreBetterFFI.xcframework.zip   # must equal release.json's checksum
shasum -a 256 WriteMoreBetterFFI.xcframework.zip                    # must equal the SHA256SUMS entry
```

`swift package compute-checksum` is the authority for `release.json`; `SHA256SUMS` is the
plain-file digest printed next to the assets.

## Roll back a bad release

Delete the release and the tag, then fix and re-tag at a new commit:

```sh
gh release delete 0.7.0 --repo cmlarsen/writemorebetter --yes
git push origin :refs/tags/0.7.0            # delete the tag
gh run view <the publish run>               # note the release.json commit; revert or fix it on main
```

**Never move a published tag.** SwiftPM pins a resolved binary target by URL and checksum,
so a tag that is re-pointed at a different commit would leave `release.json` and the asset
disagreeing. Delete the bad tag and cut a new version instead. Because the rollback leaves
`main`'s `release.json` pointing at the deleted asset, follow up with a commit that
restores the previous `release.json` (or the next version's) before anyone resolves the
package from `main`.
