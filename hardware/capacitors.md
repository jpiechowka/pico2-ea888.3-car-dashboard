# Capacitors for the historical isolated build

This shortlist belongs to the earlier Longan/Pololu wiring proposal. The
[selected USB/BLE arrangement](README.md) does not require assembling that circuit
or buying these capacitors. Ratings and unresolved checks below are retained as
dated research for the wired alternative.

Checked 2026-10-17. Use **Panasonic FR** electrolytics with identifiable part
numbers. The preferred 5 V bulk candidate is **220 µF / 50 V EEUFR1H221**:
it gives substantially lower specified impedance than the smaller 16 V part
without adding capacitance. The extra voltage rating is component margin,
not permission for the dashboard rail to exceed its limits.

These are candidates for the [module build](module-build.md). Final quantities
and placement depend on the completed protection network, converter stability
and measured inrush. Existing module capacitors count toward the total.

## Exact parts and ratings

| Role | Panasonic part | Value | Max impedance | Rated ripple | Endurance | Body / straight-lead pitch |
| --- | --- | --- | --- | --- | --- | --- |
| Protected vehicle-input bulk | **EEUFR1J101L** | 100 µF / 63 V | 68 mΩ | 741 mA RMS | 8000 h | 8 × 15 mm / 3.5 mm |
| Preferred isolated 5 V bulk | **EEUFR1H221** | 220 µF / 50 V | **30 mΩ** | **1650 mA RMS** | 8000 h | 10 × 16 mm / 5 mm |
| Smaller 5 V alternative | EEUFR1C221 | 220 µF / 16 V | 130 mΩ | 455 mA RMS | 5000 h | 6.3 × 11.2 mm / 2.5 mm |

Ratings come from the [Panasonic FR-A datasheet, pp. 1, 4 and 6](https://industrial.panasonic.com/cdbs/www-data/pdf/RDF0000/ABA0000C1259.pdf):
impedance at **100 kHz / 20°C**, ripple at **100 kHz / 105°C**, ±20% capacitance,
−40 to +105°C range. Endurance is a specified test duration, not a prediction
of installed life. The `B` suffix denotes formed/taped leads: `EEUFR1J101LB`
uses 5 mm pitch; `EEUFR1H221B` also uses 5 mm. Check the exact suffix and fit.

The 50 V candidate's tabulated impedance is about **77% lower** than the 16 V
candidate's, with the same nominal capacitance. This comparison is at the stated
test condition; impedance is not frequency-independent ESR. The larger body
needs clearance and support. Neither part replaces ceramic bypassing or a
deliberately damped input network.

## Allegro sourcing

The following offers advertised the exact codes during research. Search caches
and product pages can lag or regroup sellers; recheck the selected offer's code,
pack count and stock before ordering. Prices and seller authenticity were not
independently verified, and nothing has been ordered.

| Part | Advertised offer | Evidence notes |
| --- | --- | --- |
| EEUFR1J101L, 100 µF / 63 V | [4-pack, HFO_PL, offer 15073483356](https://allegro.pl/oferta/4szt-kondensator-panasonic-fr-100uf-63v-8x15mm-niska-impedancja-68mr-15073483356) | Indexed offer identifies FR, 8 × 15 mm, 3.5 mm pitch; direct fetch unavailable |
| EEUFR1J101LB, formed-lead alternative | [10-pack, sm-elektronik_pl, offer 7876089142](https://allegro.pl/produkt/100uf-63v-105-8x15-low-esr-panasonic-kondensator___-10szt-977a1688-2d0b-4de4-94b9-a16c30874a5a) | Retrieved page identifies the exact `LB` code and 5 mm pitch |
| EEUFR1H221, 220 µF / 50 V | [4-pack, sm-elektronik_pl, offer 18210225906](https://allegro.pl/produkt/220uf-50v-105-low-esr-impedance-30mr-panasonic-fr-kondensator-___-4szt-7937788c-c6b6-4b0a-9027-8b8eea25fc78) | Indexed offer identifies the exact code; refresh availability |
| EEUFR1H221B, formed-lead alternative | [2-pack, HFO_PL, offer 15042394812](https://allegro.pl/oferta/2szt-kondensator-panasonic-fr-220uf-50v-10x13mm-niska-impedancja-30mr-15042394812) | Exact code appears, but description also contains conflicting FC and 13 mm wording; confirm FR, 10 × 16 mm before buying |
| EEUFR1C221, space-saving alternative | [5-pack, HFO_PL, offer 15042372037](https://allegro.pl/produkt/5szt-kondensator-panasonic-fr-220uf-16v-6-3x11mm-niska-impedancja-130mr-8327125b-6928-4380-8d8b-c15841751257?offerId=15042372037) | Indexed exact code; less favorable impedance/ripple ratings |

Some listings say −55°C, while the current manufacturer's sheet says −40°C.
Some manufacturer web tables also put an Ω numeric value under an mΩ heading;
use the PDF's units. Do not substitute similarly named FC parts or generic
“low ESR” stock without checking the actual datasheet. An authorized distributor
is the fallback when the advertised identity cannot be resolved.

## Where each type belongs

| Node / purpose | Initial selection | Conditions |
| --- | --- | --- |
| Protected input, near Pololu VIN/GND | One 100 µF / 63 V candidate | Downstream of the selected reverse/transient protection; final clamp peak must fit both capacitor and regulator. A 63 V capacitor does not protect a 50 V regulator |
| Isolated output, near converter/load supply terminals | One 220 µF / 50 V candidate | Check converter startup, stability and total output capacitance, including installed boards. Keep its return entirely on dashboard ground |
| New local IC/module bypass | Around 100 nF X7R ceramic where needed | Place at supply pins with a short loop; many assembled modules already include it |
| Intermediate local storage | Module-specified parts, usually 1–10 µF effective | Keep fitted parts; check DC-bias loss and regulator requirements before adding more |

The UART Click candidate already has **100 nF plus 10 µF on each side**;
those are not extra shopping quantities. See its [schematic](https://download.mikroe.com/documents/add-on-boards/click/digi-isolator-3-click/digi-isolator-3-click-schematic.pdf).
An identifiable hand-solderable bypass option is **TDK FG28X7R1H104KNT06**,
100 nF / 50 V X7R, 5 mm lead pitch: [manufacturer specification](https://product.tdk.com/en/search/capacitor/ceramic/lead-mlcc/info?part_no=FG28X7R1H104KNT06),
[TME listing](https://www.tme.eu/pl/details/fg28x7r1h104knt06/kondensatory-mlcc-tht/tdk/).
No exact Allegro match was verified for this ceramic. Its proposed use is local
regulated-rail bypassing, not raw OBD transient suppression.

Both electrolytics connect **across their own supply and return**, with correct
polarity. Never bridge the isolation grounds or put bulk capacitors on CAN/I2C
signals. Keep leads short, insulate exposed connections and support the bodies
against vibration without blocking their vents.

## Checks before finalizing quantities

- Check input ringing with the actual approximately 1 m OBD harness. Low-ESR
  capacitors and cable inductance can need intentional series/RC damping;
  preserve that resistance rather than replacing every part with the lowest ESR.
  [Pololu's measured LC-spike examples](https://www.pololu.com/docs/0J16/all).
- Include all fitted and added capacitors in switch inrush, converter capacitive
  load and discharge checks. Keep the approved switch-off/start-engine/switch-on
  sequence. Extra charge delays power decay after opening the switch.
- Check ripple at the actual frequencies and temperature, using manufacturer
  correction factors. Cold impedance and enclosure self-heating matter.
- Scope startup, load steps, switch bounce and USB supply order on a bench.
  Keep each protected rail within the limits of every load, not merely within
  the capacitor's much higher voltage rating.

For scale, ideal 220 µF storage supports an extra 0.5 A for only **0.22 ms**
before a 0.5 V drop (`t = C × ΔV / I`), before ESR and other losses. This is
brief smoothing, not a cranking energy reserve. The [protection plan](protection-plan.md#4-capacitors-local-bypass-local-storage-and-bulk)
retains the separate fuse, reverse-polarity, overvoltage and CAN checks.
