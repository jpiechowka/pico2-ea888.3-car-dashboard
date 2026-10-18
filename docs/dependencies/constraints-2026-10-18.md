# Resolved dependency constraints, 18 October 2026

This inventory compares the five maintained workspace graphs with all optional features included with current stable
crates.io metadata. Every parent requirement below comes from locked Cargo metadata
and the published or local manifest. `cargo update --dry-run` selected no further
compatible update after libc 0.2.191. A higher version in this table is not permission
to override an upstream semver contract. Multiple incompatible API families coexist.
The four pinned standalone build tools were also included in the release lookup;
the table concerns the application's five workspace graphs.

`fixed` 1.29 and `az` 1.2 share a resolver constraint: embedded-graphics requires
az `~1.2.0`; later fixed releases require az 1.3. The cryptographic dependencies
remain in the mutually compatible versions selected by Trouble and bt-hci.
Published Embassy RP still declares PIO 0.3; the local reviewed backport selects
PIO 0.4 and is authoritative for this repository. Historical tables stay dated.

| Crate | Resolved | Latest stable | Workspace | Direct parent requirement |
| --- | --- | --- | --- | --- |
| aes | 0.8.4 | 0.9.3 | firmware | `trouble-host 0.8.0` `^0.8.2` |
| anes | 0.1.6 | 0.2.1 | host | [criterion 0.8.2](https://docs.rs/crate/criterion/0.8.2/source/Cargo.toml) `^0.1.4` |
| az | 1.2.1 | 1.3.0 | configurator, firmware, host, root | [embedded-graphics 0.8.2](https://docs.rs/crate/embedded-graphics/0.8.2/source/Cargo.toml) `~1.2.0`; [embedded-graphics-core 0.4.1](https://docs.rs/crate/embedded-graphics-core/0.4.1/source/Cargo.toml) `~1.2.0`; [fixed 1.29.0](https://docs.rs/crate/fixed/1.29.0/source/Cargo.toml) `^1.2` |
| bare-metal | 0.2.5 | 1.0.0 | firmware | [cortex-m 0.7.9](https://docs.rs/crate/cortex-m/0.7.9/source/Cargo.toml) `^0.2.4` |
| base16ct | 0.2.0 | 1.0.0 | firmware | [elliptic-curve 0.13.8](https://docs.rs/crate/elliptic-curve/0.13.8/source/Cargo.toml) `^0.2`; [sec1 0.7.3](https://docs.rs/crate/sec1/0.7.3/source/Cargo.toml) `^0.2` |
| base64 | 0.22.1 | 0.23.1 | configurator | [leptos_server 0.8.8](https://docs.rs/crate/leptos_server/0.8.8/source/Cargo.toml) `^0.22`; [server_fn 0.8.13](https://docs.rs/crate/server_fn/0.8.13/source/Cargo.toml) `^0.22` |
| bit-set | 0.8.0 | 0.11.1 | firmware | [lalrpop 0.22.2](https://docs.rs/crate/lalrpop/0.22.2/source/Cargo.toml) `^0.8` |
| bit-vec | 0.8.0 | 0.10.1 | firmware | [bit-set 0.8.0](https://docs.rs/crate/bit-set/0.8.0/source/Cargo.toml) `^0.8.0` |
| bitfield | 0.13.2 | 0.20.0 | firmware | [cortex-m 0.7.9](https://docs.rs/crate/cortex-m/0.7.9/source/Cargo.toml) `^0.13.2` |
| bitflags | 1.3.2 | 2.13.2 | firmware | [defmt 1.2.0](https://docs.rs/crate/defmt/1.2.0/source/Cargo.toml) `^1` |
| block-buffer | 0.10.4 | 0.12.1 | configurator, firmware | [digest 0.10.7](https://docs.rs/crate/digest/0.10.7/source/Cargo.toml) `^0.10` |
| bt-hci | 0.10.1 | 0.11.0 | firmware | `trouble-host 0.8.0` `^0.10` |
| cipher | 0.4.4 | 0.5.2 | firmware | [aes 0.8.4](https://docs.rs/crate/aes/0.8.4/source/Cargo.toml) `^0.4.2`; [cmac 0.7.2](https://docs.rs/crate/cmac/0.7.2/source/Cargo.toml) `^0.4.2` |
| cmac | 0.7.2 | 0.8.0 | firmware | `trouble-host 0.8.0` `^0.7.2` |
| cobs | 0.3.0 | 0.5.1 | configurator, firmware, host, root, studio-helper | [postcard 1.1.3](https://docs.rs/crate/postcard/1.1.3/source/Cargo.toml) `^0.3.0` |
| codespan-reporting | 0.11.1 | 0.13.1 | firmware | [pio-proc 0.4.0](https://docs.rs/crate/pio-proc/0.4.0/source/Cargo.toml) `^0.11` |
| color_quant | 1.1.0 | 2.0.0 | configurator, root | [gif 0.14.2](https://docs.rs/crate/gif/0.14.2/source/Cargo.toml) `^1.1` |
| const-oid | 0.9.6 | 0.10.2 | firmware | [der 0.7.10](https://docs.rs/crate/der/0.7.10/source/Cargo.toml) `^0.9.2` |
| convert_case | 0.11.0 | 0.12.0 | configurator | [convert_case_extras 0.2.0](https://docs.rs/crate/convert_case_extras/0.2.0/source/Cargo.toml) `^0.11.0`; [leptos_macro 0.8.19](https://docs.rs/crate/leptos_macro/0.8.19/source/Cargo.toml) `^0.11`; [reactive_stores_macro 0.4.4](https://docs.rs/crate/reactive_stores_macro/0.4.4/source/Cargo.toml) `^0.11`; [server_fn_macro 0.8.12](https://docs.rs/crate/server_fn_macro/0.8.12/source/Cargo.toml) `^0.11` |
| convert_case | 0.6.0 | 0.12.0 | configurator | [config 0.15.27](https://docs.rs/crate/config/0.15.27/source/Cargo.toml) `^0.6.0` |
| cpufeatures | 0.2.17 | 0.3.1 | configurator, firmware | [aes 0.8.4](https://docs.rs/crate/aes/0.8.4/source/Cargo.toml) `^0.2`; [keccak 0.1.6](https://docs.rs/crate/keccak/0.1.6/source/Cargo.toml) `^0.2`; [sha2 0.10.9](https://docs.rs/crate/sha2/0.10.9/source/Cargo.toml) `^0.2` |
| crc-any | 2.5.1 | 3.1.0 | firmware | [rp2040-boot2 0.3.0](https://docs.rs/crate/rp2040-boot2/0.3.0/source/Cargo.toml) `^2.3` |
| crypto-bigint | 0.5.5 | 0.7.5 | firmware | [elliptic-curve 0.13.8](https://docs.rs/crate/elliptic-curve/0.13.8/source/Cargo.toml) `^0.5` |
| crypto-common | 0.1.7 | 0.2.2 | configurator, firmware | [cipher 0.4.4](https://docs.rs/crate/cipher/0.4.4/source/Cargo.toml) `^0.1.6`; [digest 0.10.7](https://docs.rs/crate/digest/0.10.7/source/Cargo.toml) `^0.1.3` |
| darling | 0.20.11 | 0.24.1 | firmware | [embassy-executor-macros 0.8.0](https://docs.rs/crate/embassy-executor-macros/0.8.0/source/Cargo.toml) `^0.20.1` |
| darling_core | 0.20.11 | 0.24.1 | firmware | [darling 0.20.11](https://docs.rs/crate/darling/0.20.11/source/Cargo.toml) `=0.20.11`; [darling_macro 0.20.11](https://docs.rs/crate/darling_macro/0.20.11/source/Cargo.toml) `=0.20.11` |
| darling_macro | 0.20.11 | 0.24.1 | firmware | [darling 0.20.11](https://docs.rs/crate/darling/0.20.11/source/Cargo.toml) `=0.20.11` |
| dbl | 0.3.2 | 0.5.0 | firmware | [cmac 0.7.2](https://docs.rs/crate/cmac/0.7.2/source/Cargo.toml) `^0.3` |
| defmt | 0.3.101 | 1.2.0 | firmware | [btuuid 0.1.1](https://docs.rs/crate/btuuid/0.1.1/source/Cargo.toml) `^0.3` |
| der | 0.7.10 | 0.8.2 | firmware | [sec1 0.7.3](https://docs.rs/crate/sec1/0.7.3/source/Cargo.toml) `^0.7` |
| digest | 0.10.7 | 0.11.3 | configurator, firmware | [cmac 0.7.2](https://docs.rs/crate/cmac/0.7.2/source/Cargo.toml) `^0.10.3`; [elliptic-curve 0.13.8](https://docs.rs/crate/elliptic-curve/0.13.8/source/Cargo.toml) `^0.10`; [hmac 0.12.1](https://docs.rs/crate/hmac/0.12.1/source/Cargo.toml) `^0.10.3`; [sha2 0.10.9](https://docs.rs/crate/sha2/0.10.9/source/Cargo.toml) `^0.10.7`; [sha3 0.10.9](https://docs.rs/crate/sha3/0.10.9/source/Cargo.toml) `^0.10.7` |
| elliptic-curve | 0.13.8 | 0.14.1 | firmware | [p256 0.13.2](https://docs.rs/crate/p256/0.13.2/source/Cargo.toml) `^0.13.1`; [primeorder 0.13.6](https://docs.rs/crate/primeorder/0.13.6/source/Cargo.toml) `^0.13.7` |
| embassy-hal-internal | 0.4.0 | 0.5.0 | firmware | [embassy-embedded-hal 0.6.0](https://docs.rs/crate/embassy-embedded-hal/0.6.0/source/Cargo.toml) `^0.4.0` |
| embedded-hal | 0.2.7 | 1.0.0 | firmware | [cortex-m 0.7.9](https://docs.rs/crate/cortex-m/0.7.9/source/Cargo.toml) `^0.2.4`; [embassy-embedded-hal 0.6.0](https://docs.rs/crate/embassy-embedded-hal/0.6.0/source/Cargo.toml) `^0.2.6`; [embassy-time 0.5.1](https://docs.rs/crate/embassy-time/0.5.1/source/Cargo.toml) `^0.2.6`; `embassy-rp 0.10.0` `^0.2.6` |
| embedded-io | 0.6.1 | 0.7.1 | firmware | [embedded-io-async 0.6.1](https://docs.rs/crate/embedded-io-async/0.6.1/source/Cargo.toml) `^0.6` |
| embedded-io-async | 0.6.1 | 0.7.0 | firmware | [embassy-usb-driver 0.2.2](https://docs.rs/crate/embassy-usb-driver/0.2.2/source/Cargo.toml) `^0.6.1` |
| ff | 0.13.1 | 0.14.0 | firmware | [elliptic-curve 0.13.8](https://docs.rs/crate/elliptic-curve/0.13.8/source/Cargo.toml) `^0.13`; [group 0.13.0](https://docs.rs/crate/group/0.13.0/source/Cargo.toml) `^0.13` |
| fixed | 1.29.0 | 1.31.0 | firmware | [cyw43-pio 0.10.0](https://docs.rs/crate/cyw43-pio/0.10.0/source/Cargo.toml) `^1.23.1`; `embassy-rp 0.10.0` `^1.28.0` |
| float-cmp | 0.9.0 | 0.10.0 | configurator, firmware, host, root | [embedded-graphics 0.8.2](https://docs.rs/crate/embedded-graphics/0.8.2/source/Cargo.toml) `^0.9.0` |
| generic-array | 0.14.7 | 1.4.5 | configurator, firmware | [block-buffer 0.10.4](https://docs.rs/crate/block-buffer/0.10.4/source/Cargo.toml) `^0.14`; [crypto-bigint 0.5.5](https://docs.rs/crate/crypto-bigint/0.5.5/source/Cargo.toml) `^0.14`; [crypto-common 0.1.7](https://docs.rs/crate/crypto-common/0.1.7/source/Cargo.toml) `=0.14.7`; [dbl 0.3.2](https://docs.rs/crate/dbl/0.3.2/source/Cargo.toml) `^0.14`; [elliptic-curve 0.13.8](https://docs.rs/crate/elliptic-curve/0.13.8/source/Cargo.toml) `^0.14.6`; [inout 0.1.4](https://docs.rs/crate/inout/0.1.4/source/Cargo.toml) `^0.14`; [sec1 0.7.3](https://docs.rs/crate/sec1/0.7.3/source/Cargo.toml) `^0.14.7` |
| gloo-net | 0.6.0 | 0.7.0 | configurator | [server_fn 0.8.13](https://docs.rs/crate/server_fn/0.8.13/source/Cargo.toml) `^0.6` |
| gloo-utils | 0.2.0 | 0.3.0 | configurator | [gloo-net 0.6.0](https://docs.rs/crate/gloo-net/0.6.0/source/Cargo.toml) `^0.2` |
| group | 0.13.0 | 0.14.0 | firmware | [elliptic-curve 0.13.8](https://docs.rs/crate/elliptic-curve/0.13.8/source/Cargo.toml) `^0.13` |
| hash32 | 0.3.1 | 1.0.0 | configurator, firmware, host, root, studio-helper | [heapless 0.9.3](https://docs.rs/crate/heapless/0.9.3/source/Cargo.toml) `^0.3.0` |
| hkdf | 0.12.4 | 0.13.0 | firmware | [elliptic-curve 0.13.8](https://docs.rs/crate/elliptic-curve/0.13.8/source/Cargo.toml) `^0.12.1` |
| hmac | 0.12.1 | 0.13.0 | firmware | [hkdf 0.12.4](https://docs.rs/crate/hkdf/0.12.4/source/Cargo.toml) `^0.12.1` |
| html-escape | 0.2.15 | 0.3.0 | configurator | [leptos_macro 0.8.19](https://docs.rs/crate/leptos_macro/0.8.19/source/Cargo.toml) `^0.2`; [tachys 0.2.19](https://docs.rs/crate/tachys/0.2.19/source/Cargo.toml) `^0.2` |
| inout | 0.1.4 | 0.2.4 | firmware | [cipher 0.4.4](https://docs.rs/crate/cipher/0.4.4/source/Cargo.toml) `^0.1` |
| itertools | 0.13.0 | 0.15.0 | host | [criterion 0.8.2](https://docs.rs/crate/criterion/0.8.2/source/Cargo.toml) `^0.13`; [criterion-plot 0.8.2](https://docs.rs/crate/criterion-plot/0.8.2/source/Cargo.toml) `^0.13` |
| itertools | 0.14.0 | 0.15.0 | configurator, firmware | [lalrpop 0.22.2](https://docs.rs/crate/lalrpop/0.22.2/source/Cargo.toml) `^0.14`; [leptos_macro 0.8.19](https://docs.rs/crate/leptos_macro/0.8.19/source/Cargo.toml) `^0.14`; [reactive_stores 0.4.4](https://docs.rs/crate/reactive_stores/0.4.4/source/Cargo.toml) `^0.14`; [tachys 0.2.19](https://docs.rs/crate/tachys/0.2.19/source/Cargo.toml) `^0.14` |
| keccak | 0.1.6 | 0.2.2 | firmware | [sha3 0.10.9](https://docs.rs/crate/sha3/0.10.9/source/Cargo.toml) `^0.1.4` |
| konst | 0.2.20 | 0.4.3 | configurator | [const_format 0.2.36](https://docs.rs/crate/const_format/0.2.36/source/Cargo.toml) `^0.2.20` |
| lalrpop | 0.22.2 | 0.23.1 | firmware | [pio-parser 0.4.0](https://docs.rs/crate/pio-parser/0.4.0/source/Cargo.toml) `^0.22.0` |
| lalrpop-util | 0.22.2 | 0.23.1 | firmware | [lalrpop 0.22.2](https://docs.rs/crate/lalrpop/0.22.2/source/Cargo.toml) `^0.22.2`; [pio-parser 0.4.0](https://docs.rs/crate/pio-parser/0.4.0/source/Cargo.toml) `^0.22.0`; [pio-proc 0.4.0](https://docs.rs/crate/pio-proc/0.4.0/source/Cargo.toml) `^0.22.0` |
| mach2 | 0.5.0 | 0.7.0 | studio-helper | [io-kit-sys 0.5.0](https://docs.rs/crate/io-kit-sys/0.5.0/source/Cargo.toml) `^0.5` |
| matchit | 0.8.4 | 0.9.2 | studio-helper | [axum 0.8.9](https://docs.rs/crate/axum/0.8.9/source/Cargo.toml) `=0.8.4` |
| miniz_oxide | 0.8.9 | 0.9.1 | configurator, root | [png 0.18.1](https://docs.rs/crate/png/0.18.1/source/Cargo.toml) `^0.8` |
| nb | 0.1.3 | 1.1.0 | firmware | [embedded-hal 0.2.7](https://docs.rs/crate/embedded-hal/0.2.7/source/Cargo.toml) `^0.1.3` |
| p256 | 0.13.2 | 0.14.0 | firmware | `trouble-host 0.8.0` `^0.13.2` |
| petgraph | 0.7.1 | 0.8.3 | firmware | [lalrpop 0.22.2](https://docs.rs/crate/lalrpop/0.22.2/source/Cargo.toml) `^0.7` |
| phf_shared | 0.11.3 | 0.14.0 | firmware | [string_cache 0.8.9](https://docs.rs/crate/string_cache/0.8.9/source/Cargo.toml) `^0.11` |
| primeorder | 0.13.6 | 0.14.0 | firmware | [p256 0.13.2](https://docs.rs/crate/p256/0.13.2/source/Cargo.toml) `^0.13` |
| r-efi | 6.0.0 | 7.1.0 | configurator, studio-helper | [getrandom 0.4.3](https://docs.rs/crate/getrandom/0.4.3/source/Cargo.toml) `^6` |
| rand | 0.8.8 | 0.10.3 | firmware | `trouble-host 0.8.0` `^0.8` |
| rand_chacha | 0.3.1 | 0.10.0 | firmware | `trouble-host 0.8.0` `^0.3` |
| rand_core | 0.6.4 | 0.10.1 | firmware | [crypto-bigint 0.5.5](https://docs.rs/crate/crypto-bigint/0.5.5/source/Cargo.toml) `^0.6.4`; [elliptic-curve 0.13.8](https://docs.rs/crate/elliptic-curve/0.13.8/source/Cargo.toml) `^0.6.4`; [ff 0.13.1](https://docs.rs/crate/ff/0.13.1/source/Cargo.toml) `^0.6`; [group 0.13.0](https://docs.rs/crate/group/0.13.0/source/Cargo.toml) `^0.6`; [rand 0.8.8](https://docs.rs/crate/rand/0.8.8/source/Cargo.toml) `^0.6.0`; [rand_chacha 0.3.1](https://docs.rs/crate/rand_chacha/0.3.1/source/Cargo.toml) `^0.6.0`; `embassy-rp 0.10.0` `^0.6`; `trouble-host 0.8.0` `^0.6` |
| rand_core | 0.9.5 | 0.10.1 | firmware | `embassy-rp 0.10.0` `^0.9` |
| redox_syscall | 0.5.18 | 0.9.5 | firmware | [parking_lot_core 0.9.12](https://docs.rs/crate/parking_lot_core/0.9.12/source/Cargo.toml) `^0.5` |
| rustc_version | 0.2.3 | 0.4.1 | firmware | [bare-metal 0.2.5](https://docs.rs/crate/bare-metal/0.2.5/source/Cargo.toml) `^0.2.3` |
| sec1 | 0.7.3 | 0.8.1 | firmware | [elliptic-curve 0.13.8](https://docs.rs/crate/elliptic-curve/0.13.8/source/Cargo.toml) `^0.7.1` |
| semver | 0.9.0 | 1.0.28 | firmware | [rustc_version 0.2.3](https://docs.rs/crate/rustc_version/0.2.3/source/Cargo.toml) `^0.9` |
| semver-parser | 0.7.0 | 0.10.3 | firmware | [semver 0.9.0](https://docs.rs/crate/semver/0.9.0/source/Cargo.toml) `^0.7.0` |
| serde_qs | 0.15.0 | 1.1.3 | configurator | [leptos 0.8.22](https://docs.rs/crate/leptos/0.8.22/source/Cargo.toml) `^0.15`; [server_fn 0.8.13](https://docs.rs/crate/server_fn/0.8.13/source/Cargo.toml) `^0.15` |
| sha2 | 0.10.9 | 0.11.1 | configurator | [wasm_split_macros 0.2.2](https://docs.rs/crate/wasm_split_macros/0.2.2/source/Cargo.toml) `^0.10.8` |
| sha3 | 0.10.9 | 0.12.0 | firmware | [lalrpop 0.22.2](https://docs.rs/crate/lalrpop/0.22.2/source/Cargo.toml) `^0.10` |
| string_cache | 0.8.9 | 0.11.0 | firmware | [lalrpop 0.22.2](https://docs.rs/crate/lalrpop/0.22.2/source/Cargo.toml) `^0.8` |
| syn | 2.0.119 | 3.0.9 | configurator, firmware, host | [cortex-m-macros 0.7.8](https://docs.rs/crate/cortex-m-macros/0.7.8/source/Cargo.toml) `^2.0.117`; [darling_core 0.20.11](https://docs.rs/crate/darling_core/0.20.11/source/Cargo.toml) `^2.0.15`; [darling_macro 0.20.11](https://docs.rs/crate/darling_macro/0.20.11/source/Cargo.toml) `^2.0.15`; [embassy-executor-macros 0.8.0](https://docs.rs/crate/embassy-executor-macros/0.8.0/source/Cargo.toml) `^2.0.15`; [num_enum_derive 0.7.6](https://docs.rs/crate/num_enum_derive/0.7.6/source/Cargo.toml) `^2`; [pin-project-internal 1.1.13](https://docs.rs/crate/pin-project-internal/1.1.13/source/Cargo.toml) `^2.0.1`; [pio-proc 0.4.0](https://docs.rs/crate/pio-proc/0.4.0/source/Cargo.toml) `^2.0`; [reactive_stores_macro 0.4.4](https://docs.rs/crate/reactive_stores_macro/0.4.4/source/Cargo.toml) `^2.0`; [thiserror-impl 1.0.69](https://docs.rs/crate/thiserror-impl/1.0.69/source/Cargo.toml) `^2.0.87`; [tracing-attributes 0.1.31](https://docs.rs/crate/tracing-attributes/0.1.31/source/Cargo.toml) `^2.0`; [typed-builder-macro 0.23.2](https://docs.rs/crate/typed-builder-macro/0.23.2/source/Cargo.toml) `^2`; [wasm_split_macros 0.2.2](https://docs.rs/crate/wasm_split_macros/0.2.2/source/Cargo.toml) `^2.0.108`; [zerocopy-derive 0.8.63](https://docs.rs/crate/zerocopy-derive/0.8.63/source/Cargo.toml) `^2.0.46` |
| thiserror | 1.0.69 | 2.0.21 | configurator | [gloo-net 0.6.0](https://docs.rs/crate/gloo-net/0.6.0/source/Cargo.toml) `^1.0` |
| thiserror-impl | 1.0.69 | 2.0.21 | configurator | [thiserror 1.0.69](https://docs.rs/crate/thiserror/1.0.69/source/Cargo.toml) `=1.0.69` |
| unicode-width | 0.1.14 | 0.2.2 | firmware | [codespan-reporting 0.11.1](https://docs.rs/crate/codespan-reporting/0.11.1/source/Cargo.toml) `^0.1` |
| wasi | 0.11.1+wasi-snapshot-preview1 | 0.14.7+wasi-0.2.4 | studio-helper | [mio 1.2.4](https://docs.rs/crate/mio/1.2.4/source/Cargo.toml) `^0.11.0` |
| wasm-streams | 0.5.0 | 0.7.0 | configurator | [server_fn 0.8.13](https://docs.rs/crate/server_fn/0.8.13/source/Cargo.toml) `^0.5` |
| weezl | 0.1.12 | 0.2.1 | configurator, root | [gif 0.14.2](https://docs.rs/crate/gif/0.14.2/source/Cargo.toml) `^0.1.10` |
| windows-link | 0.2.1 | 0.100.0 | configurator, firmware, host, root, studio-helper | [generator 0.8.11](https://docs.rs/crate/generator/0.8.11/source/Cargo.toml) `>=0.1, <=0.2`; [parking_lot_core 0.9.12](https://docs.rs/crate/parking_lot_core/0.9.12/source/Cargo.toml) `^0.2.0`; [windows-result 0.4.1](https://docs.rs/crate/windows-result/0.4.1/source/Cargo.toml) `^0.2.1`; [windows-sys 0.61.2](https://docs.rs/crate/windows-sys/0.61.2/source/Cargo.toml) `^0.2.1` |
| windows-result | 0.4.1 | 0.100.0 | firmware | [generator 0.8.11](https://docs.rs/crate/generator/0.8.11/source/Cargo.toml) `>=0.3.1, <=0.4` |
