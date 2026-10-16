# ECU reference for the DNUC dashboard

This library separates historical identity, source evidence, implementation and live qualification. Research was reviewed on 15 October 2026. **No new vehicle measurement, current tune parity or exact-controller support is established here.** Studio includes an offline, searchable ECU Reference workspace alongside its retained preview.

- [Signal and research catalogue](signals.md): the same bounded Rust entries rendered in Studio, including source links and unresolved definitions.
- [Communications and qualification](communications.md): read-only discovery, transport and future validation.
- [Studio usage](../../configurator/ecu-reference.md): search, filters and source links while editing views.

## Historical identity and owner facts

A historical February 2026 scan showed software **8V0906259R / 0002**, hardware **06K907425J / H13**, ODX **EV_ECM20TFS0208V0906259R / 001002**, and **CAN: UDS**. This is an identity observation, not a raw diagnostic trace or proof of the current binary after tuning. No exact TCU identification was supplied.

The owner identifies the vehicle as a 2019 EU FWD SEAT Leon Cupra 290 (5F), DNUC EA888 Gen3 / IS38, DSG7 DQ381, factory catalyst/OPF/GPF, using Shell 100 RON petrol with an E5 label. Current installed hardware and fuel composition were not independently measured. Simos 18.10 is separately owner/vendor stated. A [public VehiCAL exact software record](https://files.vehical.net/Project/Seat_Leon_5F_2.0T_8V0906259R_0002_SCG00A1000000/167) corroborates that family; its 300 PS vehicle label does not replace the owner's 290 PS identity. No binary was downloaded or compared.

Private mail, VIN, contact information, support tickets, orders and original screenshots are excluded. Public documentation contains only approved technical facts and public sources.

## Family comparisons and vendor boundaries

The [Golf GTI TCR specification](https://www.volkswagen-newsroom.com/en/golf-gti-tcr-the-driving-presentation-4573/technical-data-the-golf-gti-tcr-in-numbers-4577) provides a useful FWD/GPF/seven-speed DSG analogue. Shared power rating, hardware or engine family does not establish ECU software parity. Audi S3/TTS and generic EA888.3 tuning lists are further family leads. Tuning support and read-only measurement support are separate product claims.

CANchecked's supplied **U-DNU.TRX** remains opaque binary after its ASCII header. Its recorded SHA256 is `0e3e4f646d349b6266a27eaaa494dea6e3a905a3deafda495dc609fea7372947` (15,591 bytes). This hash identifies the supplied file; it does not authenticate a remote download or decode its contents. The [Gen2 manual](https://www.canchecked.de/updater/MFD2832-Gen2-manual.pdf) explains partially encrypted TRX information and profile naming. Filename, header and catalogue membership cannot become a DID/scaling map or enable sensors. Accessory broadcasts are not native DQ381 definitions.

## Units, fuel and warning boundaries

MAP is **absolute pressure**. Gauge boost is a derived value only when both MAP and ambient pressure are independently qualified, use matching provenance and meet bounded age/skew requirements. No numeric skew bound is established by this research. A top wire code does not by itself prove physical clipping, and PID 4F does not extend BARO 33 in the reviewed historical definition.

Measured lambda, commanded lambda and sensor voltage/current remain separate. AFR requires an explicit stoichiometric fuel assumption. RON is not ethanol percentage; an E5 label does not measure exactly 5% ethanol. Prefer dimensionless lambda while fuel composition is unknown.

The [SEAT model-family manual](https://www.seat.co.uk/datamanual-manual/leon/my20_w28/en-uk/LEON_07_19_EN.pdf), printed page 76, gives an 80–120 °C normal-driving oil context. This is not evidence for a 120 °C critical alarm. Display spans, freshness policies, reference bands and safety thresholds have different purposes. No boost, clutch-pressure, GPF, knock or fuel-pressure warning default changes follow from a family list or tuning claim.

## Catalogue maintenance

`configurator/src/ecu_reference.rs` is the single documentary dataset, embedded only in Studio. It has a closed typed schema and fixed entries, with a 256-byte UTF-8 query limit. It cannot be imported from external data, evaluated as formulas or used to construct runtime acquisition requests. The runtime catalogue in `dashboard-config` remains separate authority.

Regenerate the committed reference projection after a reviewed catalogue edit:

```powershell
cargo run --quiet --manifest-path configurator/Cargo.toml --example ecu_reference_docs | Set-Content -Encoding utf8 docs/vehicle/ecu-reference/signals.md
```

Pinned source code documents what a tool implements at that revision. Historical SAE documents on third-party hosts are historical primary-authored copies, not authenticated current standards. Exact formulas remain unresolved where sources disagree; popularity or an idle value close to one cannot settle the applicable denominator.
