# BoltFFI native target parity plan

## Summary

Implement migration parity with rust-android-gradle in small, reviewable phases. The immediate
goals are:

- explicit per-project Android, Apple, and JVM host target selection in boltffi.toml
- backward-compatible defaults when those fields are omitted
- strict pack --no-build semantics that package only the configured targets
- host JNI packaging that works with staticlib instead of requiring cdylib
- preserved current-host JVM packaging on linux-aarch64 and windows-x86_64 while cross-host JVM
  scope stays narrow
- cross-host JVM packaging support for the real Astrolabe case: macOS release jobs emitting
  both macOS and Linux desktop natives
- later, lightweight per-invocation target policy via base config + overlay config files for PR
  vs release differences

Astrolabe validation:

- Android library config currently mixes Android ABIs with desktop targets in one Gradle
  cargo.targets list.
- Release runs on macOS and intentionally emits Android + darwin-aarch64 + linux-x86-64.
- PR CI narrows Android builds to arm64 only.
- The phased rollout below covers release parity first, then CI-target overrides later.

## Public interfaces

- Add targets.android.architectures = ["arm64", "armv7", "x86_64", "x86"].
- Add targets.apple.ios_architectures = ["arm64"].
- Add targets.apple.simulator_architectures = ["arm64", "x86_64"].
- Add targets.apple.macos_architectures = ["arm64", "x86_64"].
- Add targets.java.jvm.host_targets = ["current"].
- Support JVM host alias parsing and normalize to canonical internal IDs.
- JVM host aliases: darwin-aarch64, darwin-arm64, darwin-x86-64, darwin-x86_64, linux-x86-64,
  linux-x86_64, linux-aarch64, linux-arm64, windows-x86-64, windows-x86_64.
- current resolves to the current host target and is deduped against explicit entries.
- Preserve existing behavior when new target fields are omitted.
- Later phase: add base + overlay config support, for example boltffi.toml plus
  boltffi.ci.toml, with deep-merge semantics.

## Phase 1: Android target selection and strict --no-build

- Scope: Android only.
- Add Android architecture config parsing, validation, and defaults in boltffi_cli/src/config.rs.
- Add explicit Android target resolution helpers in boltffi_cli/src/target.rs.
- Refactor Android build, pack, check, and doctor flows to use resolved Android targets instead
  of ALL_ANDROID.
- Refactor built-library discovery so Android packaging only considers the configured targets,
  never stale artifacts for unconfigured ABIs.
- Keep the current Android linker model in boltffi_cli/src/pack/android.rs: final JNI .so is
  linked from JNI glue plus Rust staticlib.
- Tighten pack android --no-build:
    - regenerate bindings/header if requested
    - require one built Rust artifact per configured Android target
    - fail with a clear missing-target list if any are absent
    - ignore stale outputs for unconfigured ABIs
- Docs: update Android config and packaging docs to describe configurable architectures and
  strict --no-build.
- Acceptance criteria:
    - omitted config still builds all four Android ABIs
    - architectures = ["arm64"] produces only arm64-v8a
    - stale x86/x86_64 artifacts do not leak into packaging
    - pack android --no-build fails if any configured ABI artifact is missing

## Phase 2: Apple slice selection

- Scope: Apple only.
- Add Apple architecture config parsing, validation, defaults, and target-resolution helpers in
  boltffi_cli/src/config.rs and boltffi_cli/src/target.rs.
- Refactor Apple build, pack, check, and doctor flows to use resolved device, simulator, and
  optional macOS slices instead of ALL_IOS and unconditional ALL_MACOS.
- Keep include_macos as the compatibility switch; macos_architectures only applies when
  include_macos = true.
- Tighten pack apple --no-build:
    - require every configured slice to already exist
    - fail clearly when a configured slice is missing
    - never include stale old slices from prior builds
- Docs: update Apple config and packaging docs to describe slice selection and include_macos
  behavior.
- Acceptance criteria:
    - omitted config preserves current iOS + simulator defaults
    - configured subsets produce matching xcframework contents only
    - stale old slices are ignored
    - pack apple --no-build validates only configured slices

## Phase 3: JVM current-host packaging and staticlib-first JNI

- Scope: host JNI packaging on the current machine only.
- Intentional simplification for reviewability and long-term maintenance:
    - Phase 3 does not attempt to reconstruct staticlib linker inputs from Cargo build-script
      outputs, target/*/build state, or other inferred workspace artifacts.
    - `pack java --no-build` is not supported in Phase 3.
    - Current-host JVM packaging in this phase always runs through the normal build flow.
    - This is a deliberate design choice, not a temporary shortcut: Java packaging-only adds too
      much artifact-state and target-directory complexity for this phase.
- Refactor pack java in boltffi_cli/src/commands/pack.rs so the final _jni shared library
  prefers Rust staticlib and links JNI glue directly against it.
- Keep cdylib as a compatibility fallback only when staticlib is unavailable.
- Add targets.java.jvm.host_targets parsing and normalization. In this phase only current-host
  packaging is allowed:
    - current may resolve to darwin-arm64, darwin-x86_64, linux-x86_64, linux-aarch64, or
      windows-x86_64 depending on the machine
    - explicit host targets may only be packaged when they equal the current machine
    - no cross-host JVM packaging yet
- Emit deterministic per-host outputs under a structured layout such as dist/java/native/<host-
  target>/.
- Preserve the existing flat current-host copy during the transition so current users do not
  break.
- Tighten pack java --no-build:
    - regenerate header/JNI glue if requested
    - reject the command for Java in this phase with a clear unsupported message
    - direct users to rerun `pack java` without `--no-build`
- Non-goals for Phase 3:
    - no heuristic fallback that scans Cargo build outputs to infer missing native-static-libs
    - no support for Java packaging-only flows in `pack java --no-build`
    - no cache or binding scheme for reusing JVM staticlib native link metadata across packaging-
      only runs
    - no cross-host JVM toolchain abstraction beyond the current machine
- Update examples and docs so they stop implying cdylib is always required.
- Acceptance criteria:
    - crate-type = ["staticlib"] is sufficient for current-host JNI packaging
    - crate-type = ["staticlib", "cdylib"] remains compatible
    - current-host JNI packaging keeps working on linux-aarch64 and windows-x86_64
    - output layout is deterministic and consumable by Gradle/Maven assembly
    - `pack java --no-build` fails clearly as unsupported in this phase
    - Java `--no-build` failure points users to rerun without `--no-build` instead of attempting
      cache lookup or heuristic inference

## Phase 4: Cross-host JVM packaging parity

- Scope: support the Astrolabe-style release case where one host emits multiple desktop native
  outputs.
- Java `pack --no-build` remains unsupported in Phase 4. This phase expands only the normal
  build/package flow for JVM targets.
- Extend targets.java.jvm.host_targets from “current host only” to “desired host outputs”.
- Long-term JVM host target ID set:
    - current
    - darwin-arm64
    - darwin-x86_64
    - linux-x86_64
    - linux-aarch64
    - windows-x86_64
- Required initial cross-host / multi-output supported set:
    - current
    - darwin-arm64
    - darwin-x86_64
    - linux-x86_64
- linux-aarch64 and windows-x86_64 remain supported as current-host targets first; cross-host
  support for them can land after the initial parity set if toolchain/linker requirements need
  separate work.
- Resolve current to the actual host target, then dedupe.
- Add a desktop toolchain abstraction for JVM native linking, similar in spirit to Android NDK
  handling but separate from it.
- All JVM build, metadata, and artifact-probe steps must resolve against the same effective Cargo
  context:
    - selected package
    - manifest path
    - toolchain selector
    - target dir
    - explicit target triple
- Windows JVM artifact naming and native-link probing must be derived from the selected build
  toolchain and reported Cargo outputs, not from the boltffi binary's own compile-time
  `target_env`.
- Explicitly support the known migration case: macOS host packaging linux-x86_64 when the Linux
  cross-toolchain is installed and configured.
- Validate toolchain availability before build/link:
    - installed Rust target
    - configured linker/toolchain for the requested host target
    - any required target-specific include/link settings for final JNI shared library
      production
- Packaging semantics:
    - build/package every configured and toolchain-supported host target
    - fail early with a precise missing-toolchain error for unsupported configured targets
    - do not silently skip configured targets
    - unsupported Java modes should be rejected before any partial packaging work starts
- Output layout:
    - keep one directory per host target under dist/java/native/<host-target>/
    - make this layout stable so external Gradle packaging can collect it into astrolabe-
      desktop-style artifacts
- Docs: add explicit guidance for macOS-to-Linux cross-host packaging and toolchain
  requirements.
- Acceptance criteria:
    - macOS can emit darwin-arm64 plus linux-x86_64 when configured correctly
    - Linux can emit current as linux-x86_64
    - ["current", "linux-x86_64"] on Linux dedupes to a single Linux output
    - configured but unsupported host targets fail with clear setup errors

## Phase 5: Base + overlay config files for per-invocation target policy

- Scope: PR-vs-release and other environment-specific target narrowing.
- Add config loading that merges:
    - base config: boltffi.toml
    - optional overlay config: for example boltffi.ci.toml or boltffi.pr.toml
- Merge semantics:
    - deep merge by section/table
    - override values replace base values only where present
    - unspecified fields inherit from base
    - arrays replace whole arrays unless there is a compelling reason to support additive merge
- Recommended CLI shape:
    - boltffi --config boltffi.toml --overlay boltffi.ci.toml ...
    - or equivalent single-purpose option if that fits the CLI style better
- Intended usage:
    - base file contains package metadata, outputs, normal target matrices
    - PR overlay narrows Android to ["arm64"]
    - release overlay can widen Android targets or desktop host targets as needed
- Do not add named profiles in this phase.
- Do not add platform-specific ad hoc target override flags in this phase.
- Docs: add examples for PR checks, local development, and release builds.
- Acceptance criteria:
    - overlay can change only targets.android.architectures while inheriting everything else
    - overlay can change only targets.java.jvm.host_targets while inheriting everything else
    - effective config is deterministic and easy to explain
    - CI can switch target policy without mutating repo-tracked config

## Test plan

- Config tests:
    - omitted target fields reproduce current defaults
    - invalid values fail with clear validation errors
    - current resolution and dedupe work correctly
    - overlay merge overrides only specified fields
- Android tests:
    - arm64-only, arm64+armv7, full default matrix
    - stale artifacts for unconfigured ABIs are ignored
    - pack android --no-build fails on missing configured targets
- Apple tests:
    - device/simulator/macOS subsets package only configured slices
    - include_macos = false excludes macOS even if macOS architectures are set
- JVM tests:
    - current-host staticlib packaging works on macOS, Linux x86_64, Linux aarch64, and
      Windows x86_64
    - cdylib fallback remains compatible
    - pack java --no-build fails clearly as unsupported in Phase 3 and remains unsupported in
      Phase 4 unless the plan is updated explicitly
    - cross-host macOS-to-Linux packaging works when the Linux toolchain is configured
    - missing cross toolchain yields a clear failure
- Reporting tests:
    - check and doctor report resolved configured targets, not fixed global lists
- Documentation/examples:
    - at least one example shows Android subset selection
    - at least one example shows structured desktop native outputs
    - docs no longer imply cdylib is always required

## Assumptions and defaults

- boltffi.toml remains the primary source of truth.
- Existing users stay source-compatible when new target fields are omitted.
- Android target selection ships before any per-invocation override support.
- Cross-host JVM packaging is a required parity phase because Astrolabe depends on macOS
  release jobs emitting Linux desktop natives.
- PR-only target narrowing is important but can safely wait until overlay-config support lands.
- Named profiles are intentionally deferred; overlay files are the first environment-variant
  mechanism.
