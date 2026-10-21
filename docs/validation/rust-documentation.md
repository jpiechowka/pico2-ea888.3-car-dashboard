# Rust documentation contracts and checks

Rust comments are part of the implementation contract. Update them with code,
including private functions, fields, variants, constants, tests, build scripts
and reviewed vendor overlays. Keep prose short and state the actual bounds,
units, ownership, errors, partial effects and state transitions.

## Writing contracts

- Describe each named input and the returned value, including `None`, sentinels,
  saturation, ordering and unavailable observations. Use the exact parameter name
  in backticks. Explain primitive values in their domain, not by restating Rust's
  type definition. Receivers inherit the owning type's state contract.
- Document errors, panics and partial mutation where applicable. Async/hardware
  operations must describe cancellation, retained ownership and completion scope.
  Unsafe APIs require a `# Safety` section with the caller's obligations.
- Fields and enum payloads need their meaning, units and constraints. Local
  variables use ordinary comments only where the algorithm or invariant needs
  explanation; `///` cannot document arbitrary expressions or primitive types.
- Document generated items in their macro or build generator. Preserve safe
  abstraction boundaries: `pin-project-lite` 0.2.17 does not accept arbitrary
  field attributes, so its generated field meanings live in the enclosing type's
  contract and ordinary source comments. Do not replace safe projection with
  handwritten unsafe code to attach field documentation.
- Link primary specifications and the locked implementation when relevant.
  Distinguish source guarantees, assumptions, software checks and physical evidence.
  A digest is not authentication; a successful build is not hardware qualification.
- Use valid Markdown, escape angle-bracket type syntax with backticks and
  disambiguate function links such as `resolve()` from modules named `resolve`.
  Avoid suppressing rustdoc warnings or hiding items to satisfy coverage.

References: [writing Rust documentation](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html),
[rustdoc lints](https://doc.rust-lang.org/rustdoc/lints.html),
[private-item rendering](https://doc.rust-lang.org/rustdoc/command-line-arguments.html#--document-private-items-show-items-that-are-not-public),
[Cargo documentation targets](https://doc.rust-lang.org/cargo/commands/cargo-doc.html),
[Serde visitor contracts](https://serde.rs/impl-deserialize.html).

## Source inventory

From `host/`:

```powershell
cargo test --locked --example rustdoc_inventory
cargo run --locked --example rustdoc_inventory -- .. --check
```

The AST inventory reads Git-tracked and new unignored `.rs` files, including
private and cfg-disabled syntax. It records file/module and defining-item docs,
fields, variants, associated items and nested declarations. Functions require
named-input mentions and a return description; explicitly unsafe functions
require a safety section. It rejects parse/read errors instead of reporting an
empty successful result. Output is JSON; `--check` fails on uncovered contracts.
File reads are capped at 4 MiB and the Git path listing at 2 MiB.

This is structural coverage, not a prose-quality or correctness proof. Imports
and impl containers inherit item contracts. Out-of-line modules are checked in
their source files. Macros are not expanded by this tool; generator templates,
computed doc attributes and generated APIs also require semantic and rendered
review. The parser is pinned to `syn` 2.0.119, independently of the Rust compiler.

The locked external `trouble-host-macros` 0.6.0 dependency drops some input docs
and emits undocumented handles/accessors for its service/server test fixtures.
The tracked fixture modules, macro invocations and fields describe their contracts;
this does not establish complete documentation of the dependency's expanded API.
Do not edit Cargo's registry cache or vendor a new dependency just to change these
test-only generated pages. Repository-owned generators remain fully in scope.

## Rendered documentation and executable examples

From the repository root in PowerShell, with the pinned toolchain:

```powershell
$env:RUSTDOCFLAGS = '-D warnings'
cargo doc --locked --workspace --lib --all-features --no-deps --document-private-items
cargo doc --locked --workspace --bins --all-features --no-deps --document-private-items --target-dir target/rustdoc-bins
cargo test --locked --workspace --all-features --doc
```

The separate binary output directory prevents Cargo's `dashboard-config` binary
from overwriting the `dashboard_config` library's documentation. Existing CLI
names remain compatible. Open `target/doc/index.html` when available, or the
individual crate's `index.html`; binary pages are under `target/rustdoc-bins/doc/`.
Inspect representative pages, signatures, fields, safety sections and navigation
in a browser in addition to compilation. A warning-free build does not establish
that every page was visually reviewed.

[CI](../../.github/workflows/ci.yml) specifies native, Wasm, helper, host/example,
HAL/radio and separate dashboard/probe documentation routes. Firmware must use
`thumbv8m.main-none-eabihf` and
`RUSTDOCFLAGS='-D warnings -C target-cpu=cortex-m33'`. Do not use `--all-features`
on firmware or combine the probe's discovery-only BLE feature set with dashboard
pairing/security. Feature-specific builds complement the source inventory;
unsupported boards and physical behavior still require separate qualification.

Native tests exercise doctests where supported. Cross-target documentation builds
render APIs but do not execute embedded examples on hardware. Keep executable
examples bounded and free of automatic device writes, network traffic or destructive
filesystem actions.

The packet-pool ownership regressions also run under [Miri](https://github.com/rust-lang/miri)
with the same nightly pin and its `miri`/`rust-src` components:

```powershell
cargo miri test --locked --manifest-path firmware/vendor/trouble-host/Cargo.toml --target x86_64-unknown-linux-gnu --lib packet_pool::tests -- --test-threads=1
```

The explicit Linux target supports interpretation from Windows. These tests keep
live packet slices across other allocations and releases; Miri checks its current
experimental aliasing model. This is neither a complete soundness proof nor a
radio, DMA or cross-core timing test.

CI also links the retained RP2040 integer-intrinsic fixture against the actual
HAL and compiler-builtins archives. It checks that 64-bit bit-count symbols come
from compiler-builtins and are not aliases of the HAL's 32-bit ROM wrappers.
The fixture has no vector table and must never be flashed or executed. This
link check does not add RP2040 board support or validate ROM behavior physically.

## Review and provenance

Read full implementations, relevant callers and existing tests before writing
behavioral claims. Record bugs separately and reproduce them with meaningful
regressions. Review docs-only changes for accidental signature/serialization or
runtime drift, especially when editing macro-generated APIs.

Vendor source retains immutable upstream/original hashes. Intentional comments or
fixes update reviewed overlay hashes and patch notes through the existing
provenance checks. The source inventory does not grant an exception for vendor,
test or generated-template code. Do not commit private audit trackers or raw device
captures as documentation evidence.
