# Game preparation

These are the preparation scripts included in the release. They verify the
supported Steam compatibility builds and prepare an OnionOS package without
changing the installed game.

This directory is only a source mirror. Run `prepare.sh` from the downloaded
release, where the runtime and compiled tools are included.

The next candidate uses the repaired SpriteBatch framework and matching ARM
serializer metadata recorded in `runtime-contract.json`. The AOT preparation
script checks that exact pair before accepting generated code.

Maintainers can build an unpublished candidate with Python 3.12 or newer:

```sh
python3 scripts/build-release-candidate.py \
  --baseline releases/v1.2.0/stardew-valley-miyoo-mini-v1.2.0.tar.gz \
  --framework /path/to/verified/MonoGame.Framework.dll \
  --verifier /path/to/compiled/PackageVerifier.exe \
  --output releases/v1.2.1
```

Run that command from the repository root. It checks the baseline archive and
framework hashes, preserves the established runtime and relinker, copies this
checkout's preparation scripts, and excludes user gamefiles and saves. It does
not create a tag, push, publish a release, or dispatch a workflow.
