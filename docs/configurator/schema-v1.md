# Configuration v1 contract

Historical wire contract, retained for explicit migration. New exports and
firmware use [version 2](schema-v2.md).

This closes the typed model used by the [design](design.md). Field/enum order
is part of the Postcard version1 protocol. A future incompatible edit requires
a new schema version and deliberate migration; Serde defaults are not a wire
migration. All JSON structs reject unknown and duplicate fields, and all enums
are externally tagged, nonrecursive and closed.

## Document and widget model

| Type | Fields in order |
| --- | --- |
| Config | version:u16, catalogue_version:u16, boot_view:u8, views:Vec<View,8>, widgets:Vec<Widget,64>, sources:[ChannelSource;8], warnings:WarningPresentation, fuel:Fuel |
| View | id:u8, name:String<14> |
| Widget | id:u16, view:u8, channel:Channel, placement:Placement, label:String<24>, display:DisplayOptions, warning:WarningSettings |
| Placement | column:u8, row:u8, span:Span |
| DisplayOptions | decimals:u8, graph:bool, average:bool, extrema:bool, trend:bool, afr:bool, graph_range:GraphRange |
| WarningSettings | enabled:bool, applicability:Applicability, mode:LimitMode, trigger:f32, clear:f32, min_load_percent:f32, on_ms:u16, off_ms:u16, message:String<20> |
| WarningPresentation | scope:WarningScope, mode:WarningMode, page_ms:u16 |

`Vec` and `String` above are heapless types. Required text must contain at least
one non-space character, must be printable ASCII, and must fit the declared
capacity. Labels additionally fit12 characters in a one-column widget. Stable
IDs are nonzero and unique in their respective pools. View order defines
encoder order; widget order within each view defines warning ordering. Empty
individual views and gaps are valid; an empty views collection is not.

Enum order:

- Channel: Boost, Oil, Water, Dsg, Iat, Egt, Battery, Lambda.
- Span: OneByOne, TwoByOne, TwoByTwo.
- GraphRange: Auto, Fixed { min:f32, max:f32 }.
- Applicability: DemoOnly, VehicleAndDemo.
- LimitMode: High, Low, LambdaDeviation.
- WarningScope: AllViews, CurrentView.
- WarningMode: CycleOne, ListThree.

Decimals are0..3. AFR is valid only for Lambda. Fixed graph endpoints must be
finite, ordered and within the channel's representable configuration domain.
The same domain bounds warning trigger/clear values; these are configuration
limits, not ECU or recommended operating limits. Initial domains: boost
-1..10bar, temperature -100..2000C, voltage0..65.535V, lambda0.01..10.
LambdaDeviation uses nonnegative fractional deviations0..2 instead of absolute
lambda thresholds and requires0<=clear<trigger plus load0..100percent, matching
the existing Context/Rule API without a fractional-unit conversion. Other channels
use High with clear<trigger or Low with clear>trigger. Lambda requires the
contextual deviation mode. Disabled rules are still structurally valid so
enabling them cannot expose previously unchecked input. Dwell is1..30000ms;
warning page interval500..10000ms. Time advances from accepted samples/wall
time, never from frames.

## Closed source records

`sources` uses the fixed Channel order. A ChannelSource contains a catalogue
definition ID, explicit access descriptor, decoder ID, evidence tier, route,
poll selection and freshness interval. Access/decoder/evidence must exactly
match that catalogue definition. They encode the request and interpretation
without permitting user-supplied expressions or self-issued qualification.
Catalogue version1 binds decoder IDs to fixed data width, byte offset,
signedness, endianness, rational scale/offset and canonical semantic, with
human-readable descriptions and pinned documentary references in the editor.

Route is DeploymentProfile, Physical11 { request:u16, response:u16 }, or Unknown.
Poll is Off or EveryMs(u16). Freshness is a nonzero u16 bounded by the channel's
reviewed cap. Catalogue entries are compatible only with their named channel;
Disabled is compatible with any channel. Validation rejects forged access,
decoder, evidence, wrong channel and CAN identifiers greater than0x7FF.

The runtime resolver returns an implemented binding, Disabled, or a specific
unavailable reason. Only the four currently implemented generic OBD sources can
produce bindings; capability discovery and the deployment qualification gate
still run. Explicit different routes cannot create additional sessions. All
source choices, including unavailable candidates, remain visible in the editor
and exports. All source data is shared by repeated widget instances.

Defaults for the remaining sources are the fixed OBD MAP-minus-ambient candidate
for Boost, Simos10C0 actual-lambda documentary candidate for Lambda, Simos2104
documentary candidate with Unknown route for DSG, and unresolved EGT with
Unknown route. All four have polling Off. These defaults carry no exact DNUC
support claim. A selected slower polling period also bounds retry admission:
retry due is no earlier than max(configured_period, existing failure_backoff),
with the existing global failure quiet period unchanged.
EveryMs is bounded by the channel floor (500ms for Boost/Lambda/Battery,
1500ms Oil,1000ms other temperatures) through10000ms inclusive. Candidate
periods only preserve a requested setting; their resolver remains unavailable.

## Fuel model

Fuel variants are EquivalentGasoline, Stoichiometric { ratio:f32 }, and
EthanolBlend(Blend). Blend fields are percent:f32, basis:BlendBasis,
composition_origin:CompositionOrigin, density_reference:DensityReference,
gasoline_stoich:f32, ethanol_stoich:f32, gasoline_density:f32,
ethanol_density:f32. BlendBasis is Mass or Volume. Densities use kg/L and are
explicit assumptions at a common reference condition, not an inferred Shell
composition. The editor explains that E5 supplies no exact percentage.

CompositionOrigin is Assumed or Measured. DensityReference is UnknownAssumed or
CommonCelsius(f32), declaring both densities at that same reference temperature.
Measured volume composition requires CommonCelsius with a finite temperature
within-30..60C; it still labels the resulting AFR as a model estimate. The
illustrative preset is Assumed with UnknownAssumed reference, never a fabricated
temperature. Mass composition does not use densities in its calculation.

Ratios are finite5..25, densities finite0.5..1.2, blend percent0..100. The
gasoline component ratio must exceed the ethanol component ratio. Volume uses
the mass conversion in the design. Validation computes the final stoichiometric
ratio once. The wrapper holds the config and that derived value privately;
deserialization never constructs the wrapper directly. On screen the short
scale label distinguishes EQ14.7, an explicit STOICH ratio and an E-percent
estimate. Canonical data, history and warnings remain lambda based.

## APIs and failure locations

The config crate exposes `default_config`, `validate`, `decode`, `encode`,
closed model/catalogue types and bounds. `ValidatedConfig::config()` returns
an immutable reference and `stoich()` returns the resolved ratio. Errors carry
a closed code, optional view/widget ID and field enum. They allocate no text.
The optional `json` feature adds bounded `from_json_bytes` and validated
`to_json` for the host compiler/editor only.

The envelope is12 bytes plus payload: ASCII EAV1, u16LE version1, u16LE payload
length, u32LE CRC-32/ISO-HDLC. CRC covers header bytes0..8 followed by payload,
skipping bytes8..12. Check vector ASCII123456789 gives0xCBF43926. Total length
must equal12+payload_length and be at most16384. Unknown versions, invalid CRC,
nonempty Postcard remainder and semantic failures reject the entire document.
Editable JSON is capped at131072 bytes before parsing.

The payload must also equal its canonical Postcard serialization. This is
checked by an allocation-free comparison flavor, with no payload-sized scratch
buffer. In Postcard1.1.3, host usize length parsing accepts longer varints than
32-bit Pico/Wasm; rejecting noncanonical encodings ensures the host cannot
approve a file that the device rejects only because of pointer width.

The Simos manifold boost candidate is a fixed two-input derivation: DID39C0
absolute MAP and DID13CA absolute ambient pressure. Neither absolute pressure
alone is a relative-boost source. Candidate polling intervals can be stored
within the same bounded channel floors, but do not enable an executable decoder.
An explicit physical route rejects 0x7DF functional OBD requests and identical
request/response IDs in addition to the 11-bit limit.

The default document preserves today's eight1x1 channels and EQ14.7 AFR in a
named Overview view. Default warning examples are enabled for DemoOnly; vehicle
applicability requires an explicit user edit and still cannot bypass source or
operating-context checks. Boot falls back to this complete document on runtime
corruption and exposes the persistent configuration fault.
