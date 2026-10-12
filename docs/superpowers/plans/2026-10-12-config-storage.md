# Configuration Storage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task by task. Steps use checkbox syntax for tracking.

**Goal:** Load the bounded configuration directly into its final static storage before adding the approved advanced widget options.

**Architecture:** An opaque `ConfigSlot` owns initialized but unpublished storage. A Postcard seed fills fields and bounded vectors, the existing canonicality and semantic validators check that storage, and only then does a shared immutable reference escape. Host callers keep the owned convenience APIs. Firmware claims one `ConstStaticCell` before starting Core1.

**Tech Stack:** Rust nightly-2026-10-12, safe application Rust, serde 1.0.229, postcard 1.1.3, heapless 0.9.3, static_cell 2.1.1, existing ARM ELF capture tooling.

## Global Constraints

- Work on the authorized master checkout. No reset, clean or flashing. Commit source, tests and product documentation; keep TODO, audit inputs, local reports and target artifacts uncommitted. Conventional commit subjects, no AI attribution or em dashes.
- Retain the current wire version, exact serialized defaults, frozen legacy imports, error classes, complete input consumption and canonical encoding checks. This is a storage change, not a format migration.
- Preserve 8 views / 64 widgets, 16 KiB EAV / 128 KiB JSON, the 10,240-byte ARM object guards and the existing 8 KiB Core0 / 4 KiB Core1 stack reserves. Later field growth requires its own measured resource admission.
- Do not add application unsafe, heap allocation, another full configuration static, whole-Config assignment/clone on the firmware load path, or a config-sized scratch encoder buffer. Do not use zeroed Rust enums or `MaybeUninit` field writes.
- Keep one shared semantic validator and default-filling implementation. Host JSON remains derived serde; the explicit seed supports the Postcard sequence representation only.
- Current source findings and rationale are in `firmware/target/power-user-expansion/config-capacity-audit.md`. Verify the exact locked dependencies used by the implementation. `StaticCell::init_with` and serde's default `deserialize_in_place` are not stack guarantees.
- Meaningful RED/GREEN tests, self-review and focused verification per change; all four workspace formatter checks before final commit. Physical high-water and interrupt/exception overlap remain unverified.

### Task 1: Opaque slot and shared bounded decoding

**Files:** Modify `crates/dashboard-config/src/{codec,defaults,validate,lib}.rs`. Add `crates/dashboard-config/src/slot.rs` and `crates/dashboard-config/tests/slot.rs`. If useful, add an internal `decode_in_place.rs` for the sequence visitors; it has no public unchecked API. Add product explanation to `docs/configurator/configuration-storage.md`.

**Interfaces:** Keep `validate(Config)`, `decode(&[u8])`, `load_at_boot(&[u8])` and `default_config()` working. Add public opaque `ConfigSlot` with `pub const fn new() -> Self` and `pub fn load_at_boot(&mut self, bytes: &[u8]) -> &BootConfig`. It exposes no storage getter, Deref, Clone, Serialize, unchecked validated conversion or mutable validated reference. The returned borrow prevents reload while that reference is still used.

Private shared helpers are `validate_ref(&Config) -> Result<f32, ValidationError>`, `empty_config() -> Config` as a const initialized placeholder, `fill_default_config(&mut Config)`, envelope inspection and streaming canonical comparison. An envelope helper returns the checked version plus payload; the slot rejects legacy versions exactly as the firmware's current-format loader does, even when a host test enables JSON. The owned JSON-enabled decoder retains legacy upgrading.

- [ ] Write slot tests before exposing the API. The initial failure must be the missing slot, not malformed fixture setup.

```rust
use dashboard_config::*;

#[test]
fn current_file_is_published_only_after_validation() {
    let expected = decode(include_bytes!("../../../configs/default.eav")).unwrap();
    let mut slot = ConfigSlot::new();
    let loaded = slot.load_at_boot(include_bytes!("../../../configs/default.eav"));
    assert_eq!(loaded.fault, None);
    assert_eq!(loaded.config, expected);
}

#[test]
fn failure_replaces_every_field_with_the_reviewed_default() {
    let expected = validate(default_config()).unwrap();
    let mut slot = ConfigSlot::new();
    let loaded = slot.load_at_boot(b"not a file");
    assert_eq!(loaded.fault, Some(ConfigError::Length));
    assert_eq!(loaded.config, expected);
}
```

- [ ] Extract the existing validator by borrow without changing order or error identity. `validate(config)` becomes `let stoich = validate_ref(&config)?; Ok(ValidatedConfig { config, stoich })`. Extract fieldwise default filling; it assigns each scalar/global/array field and uses `clear` plus individual `push` on views/widgets. The owned constructor calls the same filler on a local const placeholder. Never call the owned constructor from the slot fallback.
- [ ] Keep placeholder access private. One implementation is a `slot` child module declared from `validate.rs`, so its code can access the parent's private `ValidatedConfig` fields while callers cannot. Its private `BootConfig` contains legally initialized Rust values but no semantically validated document until publication. Document that invariant. Placeholder controls may be explicit all-None bindings; placeholders are not public defaults and must never escape.
- [ ] Factor shared envelope and canonicality checks. Preserve order: size, magic, allowed envelope version, exact length, checksum, payload parsing, no remainder, canonical reserialization, semantic validation. A malformed file must not acquire a weaker acceptance route through the new API.
- [ ] Implement `DeserializeSeed<Value = ()>` over `&mut Config`. Assign small fields one at a time in the exact derived declaration order. Use separate seeded visitors for views/widgets; check a supplied length hint against capacity, then consume and push one bounded element at a time, with push failure rejecting overflow. Do not deserialize a complete `Vec<Widget, 64>` or `Vec<View, 8>` into a temporary. Finish the Postcard deserializer and require no remainder. Serialize through the same streaming Compare helper.

```rust
// Publication sequence; the actual helpers remain private.
match decode_current_into(bytes, &mut self.storage.config.config) {
    Ok(stoich) => {
        self.storage.config.stoich = stoich;
        self.storage.fault = None;
    }
    Err(fault) => {
        fill_default_config(&mut self.storage.config.config);
        self.storage.config.stoich =
            validate_ref(&self.storage.config.config).expect("reviewed default configuration");
        self.storage.fault = Some(fault);
    }
}
&self.storage
```

- [ ] Add a current-format differential corpus: default, maximum views/widgets, nondefault controls, colors, fuel and source metadata; compare owned decoding and slot success/config/stoichiometry. For current-format failures compare exact fault classes and complete fallback. Test every truncated prefix of valid files, plus resealed payload mutations so tests reach invalid fields instead of stopping at CRC: 9 views, 65 widgets, invalid enum/text, late NaN/Infinity, bad final control binding, unsupported payload version, overlong length/integer encodings, trailing payload bytes. Current envelope with a legacy payload is rejected. Legacy firmware rejection stays covered separately from host migration.
- [ ] Reuse a local slot in success -> early failure -> late failure -> success sequences, with nondefault fields first, to prove complete fallback reset and absence of stale fields/stoichiometry. Add compile-fail documentation tests for private storage access and a second mutable load while a prior result is used afterwards.

```compile_fail
let mut slot = dashboard_config::ConfigSlot::new();
let first = slot.load_at_boot(b"");
let second = slot.load_at_boot(b"");
assert_eq!(first.fault, second.fault);
```

- [ ] Run config tests with and without JSON, existing codec/migration/editor tests, strict config Clippy and ARM/Wasm checks. Verify default JSON/EAV hashes unchanged. Capture actual ARM slot/Config/Boot/Widget sizes without raising assertions. Commit `refactor(config): decode into an opaque bounded slot`.

### Task 2: Static firmware publication and honest stack admission

**Files:** Modify `firmware/pico2/src/view_config.rs`, `firmware/scripts/check_stack_budget.py`, `firmware/scripts/test_stack_budget.py`, and `docs/configurator/configuration-storage.md`. Use existing target capture and ELF tools; keep source inventory coverage for new config files.

**Interfaces:** Consume `dashboard_config::ConfigSlot` from Task1. Keep `view_config::initialize() -> &'static BootConfig`, EMBEDDED and SHA256 semantics. The thin startup task calls initialize, spawns the runtime task and returns before that task is polled.

- [ ] Write a stack-gate regression showing that an omitted seed/visitor/fallback child or missing frame metadata cannot be silently covered by the old decoder matcher. Existing metadata truncation, alias deduplication, outlined futures and reserve-boundary tests must continue passing. Use the observed new function names once the first captured ELF exists; retain strict ambiguity/missing-symbol checks.
- [ ] Replace the static cell with one const initialized slot:

```rust
use dashboard_pico2::view_config::{BootConfig, ConfigSlot};
use static_cell::ConstStaticCell;

static CONFIGURATION: ConstStaticCell<ConfigSlot> = ConstStaticCell::new(ConfigSlot::new());

#[inline(never)]
pub fn initialize() -> &'static BootConfig {
    CONFIGURATION.take().load_at_boot(EMBEDDED)
}
```

- [ ] Build unstripped current Demo 280/70 and Vehicle maximum configurations using the existing capture workflow. Match the standard Build-Dashboard.ps1 features: `demo,cpu280-spi70-1v30,frame-trace,te-trace` and `longan-rx12,cpu280-spi70-1v30,frame-trace,te-trace`, default simple outline and parallel rendering. Use the maximum migrated configuration in `firmware/target/gauge-audit/maximum-v2.eav`. Compare against `controls5-demo280-traced` and `controls5-vehicle280-traced`; untraced captures are not equivalent. A gate failure for old symbol selection is an expected intermediate failure, not permission to skip stack admission. Record compiler, dependency/source/config identities and exact commands.
- [ ] Inspect linked `.stack_sizes` and optimized disassembly for initialize -> slot load -> header/decode seed -> views/widgets -> canonical serializer -> borrowed validator, plus every distinct fallback path. Include drop/clear/memcpy, nested element decoding and relevant outlined helpers. Record address/frame/callee evidence and conservative child allowances. Remove obsolete owned-loader assumptions only after showing they are absent from the firmware path. Deduplicate aliases and count cumulative simultaneous frames; do not sum unrelated maximum branches as if they overlap, or omit them entirely.
- [ ] Update the named-path gate from that evidence. Require the new loader, admit required seed/fallback/validation/helper paths whether inlined or emitted, and fail if a necessary call path lacks evidence. Retain task lifetime boundaries and reserve constants. The complete current config must have no full-Config return/local temporary on the target path; a single bounded Widget temporary is allowed. Report static RAM, headroom, boot/core1/dashboard admissions and change from Task5 controls captures. If resource bounds fail, fix placement/construction or escalate the concrete architectural cause; do not enlarge guards.
- [ ] Verify both final captured artifacts pass memory, flash and revised stack gates. Run the changed Python tests, config tests, host affected suites, strict firmware Clippy and all four formatter checks. Check that source capture contains all new files. Publish exact software evidence and unresolved device high-water/IRQ/physical limits in the product document. Commit `refactor(firmware): publish configuration from static storage`.
