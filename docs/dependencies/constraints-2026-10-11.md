# Dependency version constraints, 11 October 2026

Inventory first established at `94f4d9a`; later direct/test edges add no older
resolved versions. The final lock hashes and complete direct inventory are in
[the dependency review](upgrade-2026-10-11.md).

Live official crates.io metadata reviewed on 2026-10-11. Requirements come from exact locked parent manifests via Cargo metadata, cached crate Cargo.toml, or the exact release dependencies API. Parent dev-only requirements are excluded. Lockfile presence does not imply RP2350 runtime use; several constraints belong
to host/build-only code. The table retains the exact parent requirements;
selected target/kind must be checked with Cargo metadata/tree when changing an edge.

| Graph | Package | Locked | Latest stable | Locked parent constraints |
| --- | --- | --- | --- | --- |
| firmware | [az](https://crates.io/api/v1/crates/az) | 1.2.1 | 1.3.0 | [embedded-graphics 0.8.2](https://docs.rs/crate/embedded-graphics/0.8.2/source/Cargo.toml) `~1.2.0`; [embedded-graphics-core 0.4.1](https://docs.rs/crate/embedded-graphics-core/0.4.1/source/Cargo.toml) `~1.2.0`; [fixed 1.29.0](https://docs.rs/crate/fixed/1.29.0/source/Cargo.toml) `^1.2` |
| firmware | [bare-metal](https://crates.io/api/v1/crates/bare-metal) | 0.2.5 | 1.0.0 | [cortex-m 0.7.9](https://docs.rs/crate/cortex-m/0.7.9/source/Cargo.toml) `^0.2.4` |
| firmware | [bit-set](https://crates.io/api/v1/crates/bit-set) | 0.8.0 | 0.11.1 | [lalrpop 0.22.2](https://docs.rs/crate/lalrpop/0.22.2/source/Cargo.toml) `^0.8` |
| firmware | [bit-vec](https://crates.io/api/v1/crates/bit-vec) | 0.8.0 | 0.10.1 | [bit-set 0.8.0](https://docs.rs/crate/bit-set/0.8.0/source/Cargo.toml) `^0.8.0` |
| firmware | [bitfield](https://crates.io/api/v1/crates/bitfield) | 0.13.2 | 0.19.5 | [cortex-m 0.7.9](https://docs.rs/crate/cortex-m/0.7.9/source/Cargo.toml) `^0.13.2` |
| firmware | [bitflags](https://crates.io/api/v1/crates/bitflags) | 1.3.2 | 2.13.2 | [defmt 1.1.1](https://docs.rs/crate/defmt/1.1.1/source/Cargo.toml) `^1` |
| firmware | [block-buffer](https://crates.io/api/v1/crates/block-buffer) | 0.10.4 | 0.12.1 | [digest 0.10.7](https://docs.rs/crate/digest/0.10.7/source/Cargo.toml) `^0.10` |
| firmware | [codespan-reporting](https://crates.io/api/v1/crates/codespan-reporting) | 0.11.1 | 0.13.1 | [pio-proc 0.3.0](https://docs.rs/crate/pio-proc/0.3.0/source/Cargo.toml) `^0.11` |
| firmware | [cpufeatures](https://crates.io/api/v1/crates/cpufeatures) | 0.2.17 | 0.3.1 | [keccak 0.1.6](https://docs.rs/crate/keccak/0.1.6/source/Cargo.toml) `^0.2` |
| firmware | [crc-any](https://crates.io/api/v1/crates/crc-any) | 2.5.1 | 3.1.0 | [rp2040-boot2 0.3.0](https://docs.rs/crate/rp2040-boot2/0.3.0/source/Cargo.toml) `^2.3` |
| firmware | [crypto-common](https://crates.io/api/v1/crates/crypto-common) | 0.1.7 | 0.2.2 | [digest 0.10.7](https://docs.rs/crate/digest/0.10.7/source/Cargo.toml) `^0.1.3` |
| firmware | [darling](https://crates.io/api/v1/crates/darling) | 0.20.11 | 0.24.1 | [embassy-executor-macros 0.8.0](https://docs.rs/crate/embassy-executor-macros/0.8.0/source/Cargo.toml) `^0.20.1` |
| firmware | [darling_core](https://crates.io/api/v1/crates/darling_core) | 0.20.11 | 0.24.1 | [darling 0.20.11](https://docs.rs/crate/darling/0.20.11/source/Cargo.toml) `=0.20.11`; [darling_macro 0.20.11](https://docs.rs/crate/darling_macro/0.20.11/source/Cargo.toml) `=0.20.11` |
| firmware | [darling_macro](https://crates.io/api/v1/crates/darling_macro) | 0.20.11 | 0.24.1 | [darling 0.20.11](https://docs.rs/crate/darling/0.20.11/source/Cargo.toml) `=0.20.11` |
| firmware | [digest](https://crates.io/api/v1/crates/digest) | 0.10.7 | 0.11.3 | [sha3 0.10.9](https://docs.rs/crate/sha3/0.10.9/source/Cargo.toml) `^0.10.7` |
| firmware | [embassy-hal-internal](https://crates.io/api/v1/crates/embassy-hal-internal) | 0.4.0 | 0.5.0 | [embassy-embedded-hal 0.6.0](https://docs.rs/crate/embassy-embedded-hal/0.6.0/source/Cargo.toml) `^0.4.0` |
| firmware | [embedded-hal](https://crates.io/api/v1/crates/embedded-hal) | 0.2.7 | 1.0.0 | [cortex-m 0.7.9](https://docs.rs/crate/cortex-m/0.7.9/source/Cargo.toml) `^0.2.4`; [embassy-embedded-hal 0.6.0](https://docs.rs/crate/embassy-embedded-hal/0.6.0/source/Cargo.toml) `^0.2.6`; [embassy-rp 0.10.0](https://docs.rs/crate/embassy-rp/0.10.0/source/Cargo.toml) `^0.2.6`; [embassy-time 0.5.1](https://docs.rs/crate/embassy-time/0.5.1/source/Cargo.toml) `^0.2.6` |
| firmware | [embedded-io](https://crates.io/api/v1/crates/embedded-io) | 0.6.1 | 0.7.1 | [embedded-io-async 0.6.1](https://docs.rs/crate/embedded-io-async/0.6.1/source/Cargo.toml) `^0.6` |
| firmware | [embedded-io-async](https://crates.io/api/v1/crates/embedded-io-async) | 0.6.1 | 0.7.0 | [embassy-usb-driver 0.2.2](https://docs.rs/crate/embassy-usb-driver/0.2.2/source/Cargo.toml) `^0.6.1` |
| firmware | [fixed](https://crates.io/api/v1/crates/fixed) | 1.29.0 | 1.31.0 | [embassy-rp 0.10.0](https://docs.rs/crate/embassy-rp/0.10.0/source/Cargo.toml) `^1.28.0` |
| firmware | [float-cmp](https://crates.io/api/v1/crates/float-cmp) | 0.9.0 | 0.10.0 | [embedded-graphics 0.8.2](https://docs.rs/crate/embedded-graphics/0.8.2/source/Cargo.toml) `^0.9.0` |
| firmware | [generic-array](https://crates.io/api/v1/crates/generic-array) | 0.14.7 | 1.4.5 | [block-buffer 0.10.4](https://docs.rs/crate/block-buffer/0.10.4/source/Cargo.toml) `^0.14`; [crypto-common 0.1.7](https://docs.rs/crate/crypto-common/0.1.7/source/Cargo.toml) `=0.14.7` |
| firmware | [hash32](https://crates.io/api/v1/crates/hash32) | 0.3.1 | 1.0.0 | [heapless 0.9.3](https://docs.rs/crate/heapless/0.9.3/source/Cargo.toml) `^0.3.0` |
| firmware | [itertools](https://crates.io/api/v1/crates/itertools) | 0.14.0 | 0.15.0 | [lalrpop 0.22.2](https://docs.rs/crate/lalrpop/0.22.2/source/Cargo.toml) `^0.14` |
| firmware | [keccak](https://crates.io/api/v1/crates/keccak) | 0.1.6 | 0.2.2 | [sha3 0.10.9](https://docs.rs/crate/sha3/0.10.9/source/Cargo.toml) `^0.1.4` |
| firmware | [lalrpop](https://crates.io/api/v1/crates/lalrpop) | 0.22.2 | 0.23.1 | [pio-parser 0.3.0](https://docs.rs/crate/pio-parser/0.3.0/source/Cargo.toml) `^0.22.0` |
| firmware | [lalrpop-util](https://crates.io/api/v1/crates/lalrpop-util) | 0.22.2 | 0.23.1 | [lalrpop 0.22.2](https://docs.rs/crate/lalrpop/0.22.2/source/Cargo.toml) `^0.22.2`; [pio-parser 0.3.0](https://docs.rs/crate/pio-parser/0.3.0/source/Cargo.toml) `^0.22.0`; [pio-proc 0.3.0](https://docs.rs/crate/pio-proc/0.3.0/source/Cargo.toml) `^0.22.0` |
| firmware | [nb](https://crates.io/api/v1/crates/nb) | 0.1.3 | 1.1.0 | [embedded-hal 0.2.7](https://docs.rs/crate/embedded-hal/0.2.7/source/Cargo.toml) `^0.1.3` |
| firmware | [petgraph](https://crates.io/api/v1/crates/petgraph) | 0.7.1 | 0.8.3 | [lalrpop 0.22.2](https://docs.rs/crate/lalrpop/0.22.2/source/Cargo.toml) `^0.7` |
| firmware | [phf_shared](https://crates.io/api/v1/crates/phf_shared) | 0.11.3 | 0.14.0 | [string_cache 0.8.9](https://docs.rs/crate/string_cache/0.8.9/source/Cargo.toml) `^0.11` |
| firmware | [pio](https://crates.io/api/v1/crates/pio) | 0.3.0 | 0.4.0 | [embassy-rp 0.10.0](https://docs.rs/crate/embassy-rp/0.10.0/source/Cargo.toml) `^0.3` |
| firmware | [pio-core](https://crates.io/api/v1/crates/pio-core) | 0.3.0 | 0.4.0 | [pio 0.3.0](https://docs.rs/crate/pio/0.3.0/source/Cargo.toml) `^0.3.0`; [pio-parser 0.3.0](https://docs.rs/crate/pio-parser/0.3.0/source/Cargo.toml) `^0.3.0`; [pio-proc 0.3.0](https://docs.rs/crate/pio-proc/0.3.0/source/Cargo.toml) `^0.3.0` |
| firmware | [pio-parser](https://crates.io/api/v1/crates/pio-parser) | 0.3.0 | 0.4.0 | [pio-proc 0.3.0](https://docs.rs/crate/pio-proc/0.3.0/source/Cargo.toml) `^0.3.0` |
| firmware | [pio-proc](https://crates.io/api/v1/crates/pio-proc) | 0.3.0 | 0.4.0 | [pio 0.3.0](https://docs.rs/crate/pio/0.3.0/source/Cargo.toml) `^0.3.0` |
| firmware | [rand_core](https://crates.io/api/v1/crates/rand_core) | 0.6.4 | 0.10.1 | [embassy-rp 0.10.0](https://docs.rs/crate/embassy-rp/0.10.0/source/Cargo.toml) `^0.6` |
| firmware | [rand_core](https://crates.io/api/v1/crates/rand_core) | 0.9.5 | 0.10.1 | [embassy-rp 0.10.0](https://docs.rs/crate/embassy-rp/0.10.0/source/Cargo.toml) `^0.9` |
| firmware | [redox_syscall](https://crates.io/api/v1/crates/redox_syscall) | 0.5.18 | 0.9.4 | [parking_lot_core 0.9.12](https://docs.rs/crate/parking_lot_core/0.9.12/source/Cargo.toml) `^0.5` |
| firmware | [rustc_version](https://crates.io/api/v1/crates/rustc_version) | 0.2.3 | 0.4.1 | [bare-metal 0.2.5](https://docs.rs/crate/bare-metal/0.2.5/source/Cargo.toml) `^0.2.3` |
| firmware | [semver](https://crates.io/api/v1/crates/semver) | 0.9.0 | 1.0.28 | [rustc_version 0.2.3](https://docs.rs/crate/rustc_version/0.2.3/source/Cargo.toml) `^0.9` |
| firmware | [semver-parser](https://crates.io/api/v1/crates/semver-parser) | 0.7.0 | 0.10.3 | [semver 0.9.0](https://docs.rs/crate/semver/0.9.0/source/Cargo.toml) `^0.7.0` |
| firmware | [sha3](https://crates.io/api/v1/crates/sha3) | 0.10.9 | 0.12.0 | [lalrpop 0.22.2](https://docs.rs/crate/lalrpop/0.22.2/source/Cargo.toml) `^0.10` |
| firmware | [string_cache](https://crates.io/api/v1/crates/string_cache) | 0.8.9 | 0.11.0 | [lalrpop 0.22.2](https://docs.rs/crate/lalrpop/0.22.2/source/Cargo.toml) `^0.8` |
| firmware | [syn](https://crates.io/api/v1/crates/syn) | 2.0.119 | 3.0.7 | [cortex-m-macros 0.7.8](https://docs.rs/crate/cortex-m-macros/0.7.8/source/Cargo.toml) `^2.0.117`; [darling_core 0.20.11](https://docs.rs/crate/darling_core/0.20.11/source/Cargo.toml) `^2.0.15`; [darling_macro 0.20.11](https://docs.rs/crate/darling_macro/0.20.11/source/Cargo.toml) `^2.0.15`; [defmt-macros 1.1.1](https://docs.rs/crate/defmt-macros/1.1.1/source/Cargo.toml) `^2`; [embassy-executor-macros 0.8.0](https://docs.rs/crate/embassy-executor-macros/0.8.0/source/Cargo.toml) `^2.0.15`; [num_enum_derive 0.7.6](https://docs.rs/crate/num_enum_derive/0.7.6/source/Cargo.toml) `^2`; [pio-proc 0.3.0](https://docs.rs/crate/pio-proc/0.3.0/source/Cargo.toml) `^2.0`; [proc-macro-error2 2.0.1](https://docs.rs/crate/proc-macro-error2/2.0.1/source/Cargo.toml) `^2`; [tracing-attributes 0.1.31](https://docs.rs/crate/tracing-attributes/0.1.31/source/Cargo.toml) `^2.0`; [zerocopy-derive 0.8.62](https://docs.rs/crate/zerocopy-derive/0.8.62/source/Cargo.toml) `^2.0.46` |
| firmware | [unicode-width](https://crates.io/api/v1/crates/unicode-width) | 0.1.14 | 0.2.2 | [codespan-reporting 0.11.1](https://docs.rs/crate/codespan-reporting/0.11.1/source/Cargo.toml) `^0.1` |
| firmware | [windows-link](https://crates.io/api/v1/crates/windows-link) | 0.2.1 | 0.100.0 | [generator 0.8.10](https://docs.rs/crate/generator/0.8.10/source/Cargo.toml) `>=0.1, <=0.2`; [parking_lot_core 0.9.12](https://docs.rs/crate/parking_lot_core/0.9.12/source/Cargo.toml) `^0.2.0`; [windows-result 0.4.1](https://docs.rs/crate/windows-result/0.4.1/source/Cargo.toml) `^0.2.1`; [windows-sys 0.61.2](https://docs.rs/crate/windows-sys/0.61.2/source/Cargo.toml) `0.2.1` |
| firmware | [windows-result](https://crates.io/api/v1/crates/windows-result) | 0.4.1 | 0.100.0 | [generator 0.8.10](https://docs.rs/crate/generator/0.8.10/source/Cargo.toml) `>=0.3.1, <=0.4` |
| host | [anes](https://crates.io/api/v1/crates/anes) | 0.1.6 | 0.2.1 | [criterion 0.8.2](https://docs.rs/crate/criterion/0.8.2/source/Cargo.toml) `^0.1.4` |
| host | [az](https://crates.io/api/v1/crates/az) | 1.2.1 | 1.3.0 | [embedded-graphics 0.8.2](https://docs.rs/crate/embedded-graphics/0.8.2/source/Cargo.toml) `~1.2.0`; [embedded-graphics-core 0.4.1](https://docs.rs/crate/embedded-graphics-core/0.4.1/source/Cargo.toml) `~1.2.0` |
| host | [float-cmp](https://crates.io/api/v1/crates/float-cmp) | 0.9.0 | 0.10.0 | [embedded-graphics 0.8.2](https://docs.rs/crate/embedded-graphics/0.8.2/source/Cargo.toml) `^0.9.0` |
| host | [hash32](https://crates.io/api/v1/crates/hash32) | 0.3.1 | 1.0.0 | [heapless 0.9.3](https://docs.rs/crate/heapless/0.9.3/source/Cargo.toml) `^0.3.0` |
| host | [itertools](https://crates.io/api/v1/crates/itertools) | 0.13.0 | 0.15.0 | [criterion 0.8.2](https://docs.rs/crate/criterion/0.8.2/source/Cargo.toml) `^0.13`; [criterion-plot 0.8.2](https://docs.rs/crate/criterion-plot/0.8.2/source/Cargo.toml) `^0.13` |
| host | [syn](https://crates.io/api/v1/crates/syn) | 2.0.119 | 3.0.7 | [zerocopy-derive 0.8.62](https://docs.rs/crate/zerocopy-derive/0.8.62/source/Cargo.toml) `^2.0.46` |
| host | [windows-link](https://crates.io/api/v1/crates/windows-link) | 0.2.1 | 0.100.0 | [windows-sys 0.61.2](https://docs.rs/crate/windows-sys/0.61.2/source/Cargo.toml) `^0.2.1` |

The fixed constraint also involves az: fixed 1.31.0 requires ^1.3.0, whereas the graphics family requires ~1.2.0. Forcing a compatible-major update failed the resolver. These versions remain the latest compatible selections, not the globally latest releases.
