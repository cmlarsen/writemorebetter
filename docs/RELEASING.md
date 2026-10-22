# Releasing writemorebetter

Releases are cut by the repository owner (`cmlarsen`) from `main` with the **Release**
workflow. There is no tag-push trigger on purpose: the released XCFramework `url` and
`checksum` have to sit as literals inside the tagged commit's `Package.swift`, so the commit
cannot already exist when the tag is created. The workflow rewrites them, tags and releases.

Tags are plain `X.Y.Z` (no leading `v`), matching the tag style SwiftPM needs for the
binary target URL.

## What a release publishes

| Asset | Built from |
|---|---|
| `writemorebetter-data-<version>.zip` | `scripts/build-data.sh` on ubuntu-latest — the two LGPL-2.1 data binaries (`en_tokenizer.bin`, `en_rules.bin`) with `SHA256SUMS`, `README.md` and `LICENSE-LGPL-2.1.txt` |
| `WriteMoreBetterFFI.xcframework.zip` | `scripts/build-xcframework.sh` on macos-15 — the SwiftPM binary target |
| `SHA256SUMS` | sha256 of both zip archives |
| `writemorebetter` on [PyPI](https://pypi.org/project/writemorebetter/) | `.github/workflows/python.yml`, called by the Release workflow — abi3 wheels (CPython 3.9+) for Linux (manylinux x86_64), macOS (arm64) and Windows (x64), plus the sdist. Engine code only; no LGPL data |

Both jobs use the same pinned Rust toolchain (`1.99.0`), which is an input to the
reproducible artifacts. The data binaries are **not** inside the XCFramework zip: the
checker loads them from disk, so an app ships them as resources.

## Before you start

1. Put the release date in `CHANGELOG.md`'s `# <version>` section (for example
   `# 0.7.0 (2026-10-12)`) and merge it to `main` **before** dispatching. The release notes
   are that section verbatim, and the run fails if the version's section is missing.
2. Every workspace crate that ships together — `nlprule`, `build`, `swift`
   (`writemorebetter-swift`), `wasm` and `python` (`writemorebetter-python`, whose version is
   the Python package's) — must already be at `<version>` on `main`. (`srx` is versioned
   independently.) The run fails on a mismatch. A Python wheel must be paired with the data zip
   of the same version.
3. The tag `<version>` must not exist yet. The run fails if it does.
4. Dispatch from `main`. A real release refuses to run from any other ref, because the tag
   is placed on the exact commit that was built; a dry run may come from a branch (see
   below).

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
- the `python-wheels-*` and `python-sdist` artifacts and the **Python** jobs, which install each
  wheel and a wheel built from the sdist and run the binding's tests against the data,
- the **Consumer check** job, which resolves the package as a git dependency the way an app does
  and runs a tiny app that loads the data and calls `check`.

A dry run may be dispatched from a branch (`--ref <branch>`) to rehearse before the workflow
merges; only a real release refuses to run from anything but `main`. Then cut the real
release by dispatching again with `dry_run=false` from `main`:

```sh
gh workflow run release.yml --repo cmlarsen/writemorebetter --ref main \
  -f version=0.7.0 -f dry_run=false
```

## What the release run commits and tags

The `publish` job, only when `dry_run` is false:

1. checks out `github.sha` — the commit the run was dispatched from (main's tip),
2. rewrites the binary-target `url` literal in `Package.swift` to the release asset URL
   (`…/releases/download/<version>/WriteMoreBetterFFI.xcframework.zip`) and the `checksum`
   literal to the SwiftPM checksum (`swift package compute-checksum` of the zip),
3. commits it as `writemorebetter release bot <release-bot@users.noreply.github.com>`,
4. pushes that commit to `main` — if `main` moved while the artifacts were being built the
   push is rejected (non-fast-forward) and nothing is tagged or released,
5. tags the commit `<version>` and pushes the tag,
6. creates the GitHub release on the tag with both zips, `SHA256SUMS`, `NOTICE` and the
   changelog section as the notes.

Both zips also carry `NOTICE` and the third-party license texts (`licenses/`, copied from
`style/data/licenses/`, including `cmudict.txt`, `varcon.txt` and `WordNet-LICENSE`) at their
root, next to the XCFramework or the data binaries. The CMUdict, VarCon and WordNet licenses
require binary redistributions to reproduce their notices in materials provided with the
distribution, and the XCFramework compiles in the style crate's CMUdict, regional-spelling,
emotion, hidden-verbs and filler tables. SwiftPM ignores the extra files when it resolves the
binary target.

## What the release run uploads to PyPI

The `publish-pypi` job, only when `dry_run` is false and after `publish` has created the GitHub
release, downloads the `python-wheels-*` and `python-sdist` artifacts, checks every file is
`<version>`, and uploads them with
[`pypa/gh-action-pypi-publish`](https://github.com/pypa/gh-action-pypi-publish) (pinned by
SHA). It authenticates with [Trusted Publishing](https://docs.pypi.org/trusted-publishers/):
the job runs in the `pypi` GitHub environment with `id-token: write`, and PyPI trusts that
workflow and environment, so there is no API token secret. It goes last because a PyPI upload
cannot be replaced: a version on PyPI always has its tag and its data zip.

If only this job fails (for example the trusted publisher is missing), fix the cause and use
**Re-run failed jobs** on the same run; the tag and the GitHub release already exist and are
not touched again. A version already on PyPI cannot be uploaded twice, so a bad wheel is fixed
by yanking it on pypi.org and releasing a new version.

### One-time PyPI and GitHub setup

Done once by the owner, before the first real release:

1. On pypi.org, signed in as the account that should own the project, open
   **Your account → Publishing → Add a new pending publisher → GitHub** and enter:
   - PyPI project name: `writemorebetter`
   - Owner: `cmlarsen`
   - Repository name: `writemorebetter`
   - Workflow name: `release.yml`
   - Environment name: `pypi`

   The first upload creates the project and turns the pending publisher into a normal one.
2. In GitHub, **cmlarsen/writemorebetter → Settings → Environments → New environment**, name it
   `pypi`. Optionally add yourself as a required reviewer (each upload then waits for your
   approval) and restrict deployment branches to `main`.

So a release is exactly one commit on `main` that points `Package.swift` at the new asset,
plus the tag and the release pointing at it.

## Verify the published checksum

The app's binary target is verified by SwiftPM against the `checksum` literal in
`Package.swift`, but you can check the published asset by hand:

```sh
gh release download 0.7.0 --repo cmlarsen/writemorebetter -p WriteMoreBetterFFI.xcframework.zip
swift package compute-checksum WriteMoreBetterFFI.xcframework.zip   # must equal Package.swift's checksum
shasum -a 256 WriteMoreBetterFFI.xcframework.zip                    # must equal the SHA256SUMS entry
```

`swift package compute-checksum` is the authority for `Package.swift`'s checksum; `SHA256SUMS`
is the plain-file digest printed next to the assets.

## Roll back a bad release

Delete the release and the tag, then fix and re-tag at a new commit:

```sh
gh release delete 0.7.0 --repo cmlarsen/writemorebetter --yes
git push origin :refs/tags/0.7.0            # delete the tag
gh run view <the publish run>               # note the Package.swift commit; revert or fix it on main
```

**Never move a published tag.** SwiftPM pins a resolved binary target by URL and checksum,
so a tag that is re-pointed at a different commit would leave `Package.swift` and the asset
disagreeing. Delete the bad tag and cut a new version instead. Because the rollback leaves
`main`'s `Package.swift` pointing at the deleted asset, follow up with a commit that restores
the previous URL and checksum (or the next version's) before anyone resolves the package from
`main`.
