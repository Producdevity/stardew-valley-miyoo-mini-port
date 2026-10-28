# 1.2.1 candidate validation, 2026-10-28

Status: built and checked locally; unpublished. Physical Miyoo Mini Plus /
OnionOS confirmation and the platform installer matrix remain release gates.

The archive is `releases/v1.2.1/stardew-valley-miyoo-mini-v1.2.1.tar.gz`.
Its SHA-256 is
`ac5d68618118b2b5e02e5755715f8d4c224b6c93bc53856fbd145eb7959cb460`.
The setup application's release-kit metadata pins that archive. A second build
using the same inputs produced byte-identical output.

## Runtime repair

The released framework declares `SpriteBatch : IDisposable`, but compiled game
code calls inherited `GraphicsResource` members on it. That incorrect layout
can return its batch list as the graphics device; a menu clipping write then
corrupts that list. The native source repair restores the expected inheritance:

```csharp
public sealed class SpriteBatch : GraphicsResource
{
    public SpriteBatch(GraphicsDevice graphicsDevice)
    {
        GraphicsDevice = graphicsDevice;
        // Existing initialization follows.
    }
}
```

The hiding, empty `Dispose()` is removed so inherited resource disposal runs.
The isolated framework repair is pinned in `runtime-contract.json`. Serializer
AOT must be rebuilt against its new framework identity: code remains identical,
but dependency metadata changes. `scripts/compile-aot.sh` checks the matching
framework and serializer sections.

The candidate preserves the established v1.2.0 native runtime, loader and
AssemblyRelinker. It imports the isolated framework repair, matching preparation
contract and checked package verifier. It does not import the native research
checkout's other renderer or game changes. No proprietary gamefiles or saves
are included.

## Completed checks

- The native SpriteBatch compatibility regression fails against the released
  framework and passes with the isolated repair. Resource-disposal mutation
  tests and transform tests pass.
- On the RG35XX-H, the released framework crashes after the menu sequence;
  two repaired runs complete the same sequence and grass/tool/location route.
  These runs use the real game methods through a test loader. They establish
  the defect and regression on that device; they are not Miyoo gameplay proof.
- The candidate kit passes layout/runtime checks. A clean extraction prepared
  the supported, user-owned game build successfully, including relinking,
  serialization, ARM AOT compilation and final package verification. Original
  source gamefiles are unchanged.
- The four candidate-builder tests pass, including reproducibility, rejection
  of an unaccepted framework or stale AOT contract, and game/save exclusion.
- Setup frontend compilation/build and all five Node tests pass. Rust formatting,
  Clippy with warnings denied and 28 ordinary Rust tests pass; three tests
  requiring external fixtures are normally ignored. The external-game fixture
  was also run successfully against this exact archive and owned gamefiles.
- A local macOS ARM64 setup executable builds with the candidate archive staged.
  This is an unsigned local build, not a signed installer matrix result.

The native checkout retains logs under
`artifacts/cleanup-release-20261028/`, including `clean-prepare-final.log`,
`kit-verification.log`, `committed-package-tests-final.log` and
`public-builder-tests.log`. Native issue-13 device evidence and limitations are
documented in `docs/ISSUE13_SPRITEBATCH_CORRUPTION.md` there.

## Remaining release gates

1. Confirm the repaired package on the physical Miyoo Mini Plus with OnionOS:
   load existing saves, exercise Social/Quests/Robin's construction menus,
   continue through grass clearing and location transitions, save and reload.
   The reporter described intermittent crashes and supplied no deterministic
   sequence, so a successful short run does not prove every crash is resolved.
2. Build and check the intended Windows, Linux and Intel macOS installers and
   signing/notarization as applicable. The existing dispatch workflow publishes
   installers, so it has deliberately not been invoked during this local work.
3. Obtain the user's release authorization. No push, tag, GitHub release,
   workflow dispatch or issue closure was performed.

The earlier SD update changed only the tested framework, serializer sidecar and
its pairing marker, and verified existing gamefiles and saves against the backup.
That update is suitable for the manual Miyoo test. Updating an installation
must continue to preserve its gamefiles and savedata.
