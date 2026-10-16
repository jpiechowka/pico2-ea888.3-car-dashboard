# ECU signal and research catalogue

Generated from `configurator/src/ecu_reference.rs`. Research reviewed 15 October 2026. No live qualification is established. Formulas are documentary text, never executable decoders.

<a id="identity"></a>

## Historical ECU identity

**Vehicle · Reference only**

Applicability: Historical scan, February 2026; owner statements separately identified

Observed software 8V0906259R / 0002; hardware 06K907425J / H13; ODX EV_ECM20TFS0208V0906259R / 001002; CAN: UDS. Owner identifies a 2019 EU FWD SEAT Leon Cupra 290, DNUC EA888 Gen3 / IS38 / DQ381, Shell 100 RON E5. Simos 18.10 is owner/vendor stated and independently corroborated by a public exact-box-code record. Current post-tune binary, TCU identity, routes and hardware remain unverified. The public record's 300 PS label does not override the owner's 290 PS identity.

Units: Technical identity only

Documentary decoding/scaling: `No live identity or measurement claim`

- [Source 1](https://files.vehical.net/Project/Seat_Leon_5F_2.0T_8V0906259R_0002_SCG00A1000000/167)

<a id="disabled"></a>

## Disabled channel

**Signals · Reference only**

Applicability: Configuration

A disabled source makes no measurement request. Opening this reference does not change the configuration.

Units: None

Documentary decoding/scaling: `No decoder`

Dashboard binding: `Disabled`. Live qualification remains required.


<a id="coolant"></a>

## OBD coolant · 01 05

**Signals · Generic decoder implemented**

Applicability: Generic OBD; selected responder capability and deployment qualification required

Coolant temperature. Implemented generic decoding does not establish exact-car support.

Units: °C

Documentary decoding/scaling: `One unsigned byte A: A - 40`

Dashboard binding: `ObdCoolant05`. Live qualification remains required.

- [Source 1](https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/commands.py)
- [Source 2](https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/decoders.py)
- [Source 3](https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/UnitsAndScaling.py)

<a id="intake"></a>

## OBD intake temperature · 01 0F

**Signals · Generic decoder implemented**

Applicability: Generic OBD; selected responder capability and deployment qualification required

Intake air temperature; retain the source location.

Units: °C

Documentary decoding/scaling: `One unsigned byte A: A - 40`

Dashboard binding: `ObdIntake0f`. Live qualification remains required.

- [Source 1](https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/commands.py)
- [Source 2](https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/decoders.py)
- [Source 3](https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/UnitsAndScaling.py)

<a id="oil"></a>

## OBD engine oil · 01 5C

**Signals · Generic decoder implemented**

Applicability: Generic OBD; selected responder capability and deployment qualification required

Engine oil temperature. Display span and warning thresholds are separate settings.

Units: °C

Documentary decoding/scaling: `One unsigned byte A: A - 40`

Dashboard binding: `ObdOil5c`. Live qualification remains required.

- [Source 1](https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/commands.py)
- [Source 2](https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/decoders.py)
- [Source 3](https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/UnitsAndScaling.py)

<a id="voltage"></a>

## OBD ECU voltage · 01 42

**Signals · Generic decoder implemented**

Applicability: Generic OBD; selected responder capability and deployment qualification required

ECU module voltage is distinct from battery-post or adapter supply voltage.

Units: V

Documentary decoding/scaling: `Two unsigned big-endian bytes AB: AB / 1000`

Dashboard binding: `ObdVoltage42`. Live qualification remains required.

- [Source 1](https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/commands.py)
- [Source 2](https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/decoders.py)
- [Source 3](https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/UnitsAndScaling.py)

<a id="obd-boost"></a>

## MAP / BARO and derived boost · 01 0B / 33

**Signals · Unresolved conflict**

Applicability: Generic OBD; selected responder capability and deployment qualification required

MAP is absolute pressure. Historical PID 4F field d changes MAP scaling, never BARO 33. A top wire code alone proves neither clipping nor a safety threshold. Gauge boost needs separately qualified ambient pressure and bounded sample age/skew; no numeric bound is established here.

Units: kPa absolute; bar gauge only after qualification

Documentary decoding/scaling: `Default MAP A kPa; nonzero 4F d: A*10*d/255 kPa. Derived (MAP-ambient)/100 bar`

Dashboard binding: `ObdMapMinusAmbient`. Live qualification remains required.

- [Source 1](https://www.autonerdz.com/yabbfiles/Attachments/1979v3.pdf)
- [Source 2](https://github.com/fr3ts0n/AndrOBD/blob/2d4974f3095ee2594833b59372b39d175ea24d71/library/src/main/java/com/fr3ts0n/ecu/prot/obd/res/conversions.csv)

<a id="lambda24"></a>

## Measured wideband ratio / voltage · 01 24

**Signals · Unresolved conflict**

Applicability: Generic OBD; selected responder capability and deployment qualification required

Scaling unresolved. Keep first-pair ratio separate from second-pair sensor voltage. Bank/sensor and PID 4F state need qualification. Historical table 2/65535 and zero-default prose 1.999/65535 disagree; implementations also use 1/32768. Do not choose from an idle display near one.

Units: Lambda dimensionless; sensor V separately

Documentary decoding/scaling: `x=256*A+B, y=256*C+D. Historical x*2/65535; y*8/65535 V. Competing defaults: x/32768; x*1.999/65535; y*7.999/65535`

Dashboard binding: `ObdLambda24`. Live qualification remains required.

- [Source 1](https://www.autonerdz.com/yabbfiles/Attachments/1979v3.pdf)
- [Source 2](https://github.com/fr3ts0n/AndrOBD/blob/2d4974f3095ee2594833b59372b39d175ea24d71/library/src/main/java/com/fr3ts0n/ecu/prot/obd/res/conversions.csv)
- [Source 3](https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/UnitsAndScaling.py)

<a id="lambda34"></a>

## Measured wideband ratio / current · 01 34

**Signals · Unresolved conflict**

Applicability: Generic OBD; selected responder capability and deployment qualification required

Scaling unresolved for ratio as on PID 24. Current uses offset-binary input, not signed-int16 reinterpretation. PID 4F ratio and current maxima are independent; location remains unqualified.

Units: Lambda dimensionless; sensor mA separately

Documentary decoding/scaling: `Historical ratio x*2/65535 vs x/32768 and x*1.999/65535. Current (y-32768)*128/32768 mA; nonzero 4F c replaces 128`

Dashboard binding: `ObdLambda34`. Live qualification remains required.

- [Source 1](https://www.autonerdz.com/yabbfiles/Attachments/1979v3.pdf)
- [Source 2](https://github.com/fr3ts0n/AndrOBD/blob/2d4974f3095ee2594833b59372b39d175ea24d71/library/src/main/java/com/fr3ts0n/ecu/prot/obd/res/conversions.csv)

<a id="simos-coolant"></a>

## Simos coolant · 22 11CD

**Signals · Unqualified candidate**

Applicability: Family evidence; exact current ECU/TCU unqualified

Pinned logger entry; not enabled as a dashboard decoder.

Units: °C

Documentary decoding/scaling: `u8 x - 40`

Dashboard binding: `SimosCoolant11cd`. Live qualification remains required.

- [Source 1](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/logs/csv/parameters_22.csv)

<a id="simos-oil"></a>

## Simos oil · 22 202F

**Signals · Unqualified candidate**

Applicability: Family evidence; exact current ECU/TCU unqualified

Preserve the source's 2731.4 constant. Sensor and software parity remain unverified.

Units: °C

Documentary decoding/scaling: `unsigned BE16 (x-2731.4)/10`

Dashboard binding: `SimosOil202f`. Live qualification remains required.

- [Source 1](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/logs/csv/parameters_22.csv)

<a id="simos-intake"></a>

## Simos intake · 22 1001

**Signals · Unqualified candidate**

Applicability: Family evidence; exact current ECU/TCU unqualified

Intake location and calibration need qualification.

Units: °C

Documentary decoding/scaling: `u8 0.75*x-48`

Dashboard binding: `SimosIntake1001`. Live qualification remains required.

- [Source 1](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/logs/csv/parameters_22.csv)

<a id="simos-voltage"></a>

## Simos module voltage · 22 14A6

**Signals · Unqualified candidate**

Applicability: Family evidence; exact current ECU/TCU unqualified

ECU module voltage, not battery-post measurement.

Units: V

Documentary decoding/scaling: `u8 0.1015625*x (13*x/128)`

Dashboard binding: `SimosVoltage14a6`. Live qualification remains required.

- [Source 1](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/logs/csv/parameters_22.csv)

<a id="simos-boost"></a>

## Simos manifold / ambient · 39C0 / 13CA

**Signals · Unqualified candidate**

Applicability: Family evidence; exact current ECU/TCU unqualified

MAP 39C0 and requested 39C1 are manifold absolute pressure. Upstream-throttle 202A / 2029 is a different location. Derived gauge boost requires matching provenance and bounded input age/skew; no numeric bound is supplied.

Units: kPa absolute; derived bar gauge

Documentary decoding/scaling: `unsigned BE16 MAP x/10; ambient x/120.60176665439; (MAP-ambient)/100 bar`

Dashboard binding: `SimosMapMinusAmbient`. Live qualification remains required.

- [Source 1](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/logs/csv/parameters_22.csv)

<a id="simos-lambda"></a>

## Simos lambda actual / target · 10C0 / F444

**Signals · Unqualified candidate**

Applicability: Family evidence; exact current ECU/TCU unqualified

Dimensionless actual ratio and target are separate. Another family list rounds actual to 0.000977*x and uses target DID 1456; labels alone cannot prove equivalence. AFR needs an explicit fuel assumption.

Units: Lambda dimensionless

Documentary decoding/scaling: `unsigned BE16 actual x/1024; target x/32768`

Dashboard binding: `SimosLambda10c0`. Live qualification remains required.

- [Source 1](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/logs/csv/parameters_22.csv)
- [Source 2](https://github.com/bri3d/MQBSimosLogVariables/blob/747b50e1af650c39cd9c5f6b4d58905db6debb45/exportedPIDs.csv)

<a id="dsg-oil"></a>

## DSG temperature · 22 2104

**Signals · Unqualified candidate**

Applicability: Family evidence; exact current ECU/TCU unqualified

DQ381 is owner identified; exact TCU identity, route, temperature location and scale are unqualified. Historical SimosTools DSG path is little-endian, unlike its engine path. Do not merge oil, clutch and mechatronic temperatures.

Units: Source labels °C; exact meaning unresolved

Documentary decoding/scaling: `Historical signed 16-bit identity scale; not an admitted decoder`

Dashboard binding: `DsgOil2104`. Live qualification remains required.

- [Source 1](https://github.com/Switchleg1/SimosTools/blob/f99771ceb0442c863601a84a282c6e67e6b311a8/app/src/main/java/com/app/simostools/PIDs.kt)
- [Source 2](https://github.com/Switchleg1/SimosTools/blob/f99771ceb0442c863601a84a282c6e67e6b311a8/app/src/main/java/com/app/simostools/UDSlog.kt)

<a id="egt"></a>

## Exhaust temperature / GPF

**Signals · Unqualified candidate**

Applicability: Family evidence; exact current ECU/TCU unqualified

No exact manifold EGT or GPF DID/scaling established. Catalyst temperature is not interchangeable with manifold EGT or filter temperature. Measured and modelled values must remain distinct.

Units: Location-specific °C once qualified

Documentary decoding/scaling: `No qualified definition`

Dashboard binding: `UnresolvedEgt`. Live qualification remains required.

- [Source 1](https://www.ross-tech.com/vcds/tour/controller-channels-map.php)

<a id="gear-conflict"></a>

## Gear · 210F offset conflict

**Open questions · Unresolved conflict**

Applicability: Family evidence; exact current ECU/TCU unqualified

VW_Flash and SimosTools SI use x; SimosTools USA uses x+1. Selected versus engaged gear semantics remain open.

Units: Gear enum

Documentary decoding/scaling: `Competing x and x+1`

- [Source 1](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/logs/csv/parameters_22.csv)
- [Source 2](https://github.com/Switchleg1/SimosTools/blob/f99771ceb0442c863601a84a282c6e67e6b311a8/app/src/main/java/com/app/simostools/PIDs.kt)
- [Source 3](https://github.com/Switchleg1/SimosTools/blob/f99771ceb0442c863601a84a282c6e67e6b311a8/app/src/main/java/com/app/simostools/PID/PIDList22-USA.csv)

<a id="turbo-conflict"></a>

## Turbo speed · 1040 unit conflict

**Open questions · Unresolved conflict**

Applicability: Family evidence; exact current ECU/TCU unqualified

The same numeric scale is labelled rpm in VW_Flash and krpm in SimosTools USA. No measured turbo speed or warning threshold is established.

Units: rpm versus krpm unresolved

Documentary decoding/scaling: `x/163.84 with conflicting units`

- [Source 1](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/logs/csv/parameters_22.csv)
- [Source 2](https://github.com/Switchleg1/SimosTools/blob/f99771ceb0442c863601a84a282c6e67e6b311a8/app/src/main/java/com/app/simostools/PID/PIDList22-USA.csv)

<a id="valve-conflict"></a>

## Valve lift · 167C versus 3D6C

**Open questions · Unresolved conflict**

Applicability: Family evidence; exact current ECU/TCU unqualified

Two pinned family lists assign different DIDs. Do not silently merge them.

Units: Definition unresolved

Documentary decoding/scaling: `VW_Flash 167C; SimosTools USA 3D6C`

- [Source 1](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/logs/csv/parameters_22.csv)
- [Source 2](https://github.com/Switchleg1/SimosTools/blob/f99771ceb0442c863601a84a282c6e67e6b311a8/app/src/main/java/com/app/simostools/PID/PIDList22-USA.csv)

<a id="dsg-pressure-conflict"></a>

## DSG clutch pressure · 38EA / 38EB

**Open questions · Unresolved conflict**

Applicability: Family evidence; exact current ECU/TCU unqualified

Historical unsigned fields were corrected to signed in an issue attachment. The discussion concerns DQ500 and mentions DQ381; it does not qualify this TCU. The pinned DSG parser is little-endian. Vendor tuning pressure claims are not safe alarms.

Units: bar (family claim)

Documentary decoding/scaling: `x/100; unsigned versus signed 16-bit conflict`

- [Source 1](https://github.com/Switchleg1/SimosTools/blob/f99771ceb0442c863601a84a282c6e67e6b311a8/app/src/main/java/com/app/simostools/PIDs.kt)
- [Source 2](https://github.com/Switchleg1/SimosTools/blob/f99771ceb0442c863601a84a282c6e67e6b311a8/app/src/main/java/com/app/simostools/UDSlog.kt)
- [Source 3](https://github.com/Switchleg1/SimosTools/issues/7)

<a id="scaling4f"></a>

## PID 4F · independent scaling prerequisites

**Units & limits · Unresolved conflict**

Applicability: Historical SAE tables; current applicable annex unresolved

Four fields a/b/c/d are independent maxima for ratio/voltage/current/MAP. Zero selects that field's original definition. Established unsupported differs from timeout or unqueried. Keep all four bytes, responder, session and timestamps; requalify after identity/session changes. No decoder state machine is introduced here.

Units: ratio; V; mA; 10*d kPa

Documentary decoding/scaling: `Nonzero a: ratio x*a/65535; b: voltage y*b/65535; c: current (y-32768)*c/32768; d: MAP A*10*d/255`

- [Source 1](https://www.autonerdz.com/yabbfiles/Attachments/1979v3.pdf)
- [Source 2](https://github.com/fr3ts0n/AndrOBD/blob/2d4974f3095ee2594833b59372b39d175ea24d71/library/src/main/java/com/fr3ts0n/ecu/prot/obd/res/conversions.csv)

<a id="commanded-lambda"></a>

## Commanded ratio · 01 44

**Signals · Unresolved conflict**

Applicability: Generic OBD; selected responder capability and deployment qualification required

Commanded ratio is distinct from measured O2 ratio. Historical printed defaults disagree with their zero-default prose and implementations. A rounded display cannot settle denominator choice. Applicable annex/erratum or exact-controller evidence is still needed.

Units: Lambda dimensionless

Documentary decoding/scaling: `Historical x*2/65535; prose x*1.999/65535; implementations x/32768 or rounded 0.0000305*x`

- [Source 1](https://www.autonerdz.com/yabbfiles/Attachments/1979v3.pdf)
- [Source 2](https://github.com/fr3ts0n/AndrOBD/blob/2d4974f3095ee2594833b59372b39d175ea24d71/library/src/main/java/com/fr3ts0n/ecu/prot/obd/res/conversions.csv)
- [Source 3](https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/UnitsAndScaling.py)

<a id="fuel"></a>

## Fuel, lambda and AFR · 100 RON / E5

**Units & limits · Reference only**

Applicability: Owner fuel label; composition unmeasured

RON is not ethanol percentage. E5 is not an assay of exactly 5% ethanol. Lambda avoids an invented fuel-specific AFR. AFR = lambda times the configured stoichiometric ratio; composition and density assumptions must be stated. Editing fuel presentation does not qualify a sensor.

Units: Lambda; AFR mass ratio

Documentary decoding/scaling: `No measured ethanol signal`

- [Source 1](https://www.seat.co.uk/datamanual-manual/leon/my20_w28/en-uk/LEON_07_19_EN.pdf)

<a id="limits"></a>

## Display ranges, freshness and warnings

**Units & limits · Reference only**

Applicability: Model guidance and host policies

SEAT's model-family oil 80–120 °C normal-driving context is not a 120 °C critical alarm. Gauge maximum is a display span. Freshness is an acquisition policy. A tuning claim, family design rating or wire maximum cannot set a safe vehicle limit.

Units: Canonical metric values; presentation conversions

Documentary decoding/scaling: `No warning-default changes`

- [Source 1](https://www.seat.co.uk/datamanual-manual/leon/my20_w28/en-uk/LEON_07_19_EN.pdf)

<a id="transport"></a>

## CAN / ISO-TP / UDS read-only transport

**Sources · Reference only**

Applicability: Protocol documentation; exact route unverified

Read-only diagnostics transmit requests and flow control. CAN: UDS identity does not prove addresses, bitrate, padding or payload schema. Mode 01 and service 22 are distinct. Reassembly must check source, epoch, lengths and sequence. NRC 78 is pending, not a sample; timeout is not proof of unsupported. No automatic session/security unlock.

Units: Request bytes and bounded timing

Documentary decoding/scaling: `01 PID; 22 DID_hi DID_lo; positive 41 PID or 62 DID echo`

- [Source 1](https://www.kernel.org/doc/html/latest/networking/iso15765-2.html)
- [Source 2](https://udsoncan.readthedocs.io/en/latest/udsoncan/client.html)

<a id="identity-discovery"></a>

## Read-only identity and capability discovery

**Validation · Reference only**

Applicability: Future capture checklist; no vehicle connection made

Capture current engine and TCU identities separately after tuning. Family identity DID candidates F19E/F1A2/F187/F189/F191/F1A3/F197/F1AD require exact lengths and semantics. Mode 01 support pages apply only to the selected responder/current epoch. Do not retrieve or publish VIN merely for this reference.

Units: Identity / support evidence

Documentary decoding/scaling: `A scan label is not a DID map`

- [Source 1](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/lib/constants.py)
- [Source 2](https://www.ross-tech.com/vcds/tour/controller-channels-map.php)

<a id="qualification"></a>

## Qualify a value before enabling it

**Validation · Reference only**

Applicability: Four independent evidence concepts

Evidence describes the source; applicability describes which controller it concerns; implementation describes decoder availability; live qualification requires current physical evidence. Record current identity/session, responder, raw request/reply, width, byte order, signedness, units, location, sentinel/NRC behavior, timestamps and independent comparisons. Measure latency/staleness and preserve sanitized replay fixtures. No card can grant live qualification.

Units: Evidence, applicability, implementation, live qualification

Documentary decoding/scaling: `Family/tool compatibility is not exact-car support`

- [Source 1](https://www.ross-tech.com/vcds/tour/controller-channels-map.php)
- [Source 2](https://support.obdeleven.com/en/articles/11009883-what-is-live-data)

<a id="trx"></a>

## CANchecked U-DNU.TRX · opaque vendor profile

**Sources · Reference only**

Applicability: DNU-family lead only

Supplied TRX has an ASCII header followed by opaque binary. Its contents are not a decoded DID/scale map. Filename and catalogue presence do not qualify DNUC sensors. CANchecked product transport settings and DSG accessory broadcasts cannot be copied as native DQ381 definitions.

Units: Unknown until decoded and qualified

Documentary decoding/scaling: `No recovered raw DID/scaling table`

- [Source 1](https://www.canchecked.de/updater/MFD2832-Gen2-manual.pdf)
- [Source 2](https://www.canchecked.de/downloads/uds-einrichtung/)

<a id="comparisons"></a>

## EA888 / Simos / DQ381 family comparisons

**Vehicle · Reference only**

Applicability: Family evidence; exact current ECU/TCU unqualified

Golf GTI TCR FWD/GPF/DSG is a useful 290 PS analogue. Audi S3/TTS, shared 06K907425J hardware and Simos family labels do not prove exact software parity. Tuning-tool support differs by box code and product. Advanced RAM logging can require patches and is outside this dashboard's read-only constructors.

Units: Family context only

Documentary decoding/scaling: `No shared-hardware decoder inference`

- [Source 1](https://www.volkswagen-newsroom.com/en/golf-gti-tcr-the-driving-presentation-4573/technical-data-the-golf-gti-tcr-in-numbers-4577)
- [Source 2](https://docs.vehical.net/logger_basicadvanced)

<a id="fuel-pressure"></a>

## Fuel pressure actual / requested

**Signals · Unqualified candidate**

Applicability: Family evidence; exact current ECU/TCU unqualified

Direct fuel pressure 2027/293B and low-side 2025/2932 are different sources. Compare qualified synchronized inputs; do not borrow thresholds.

Units: bar

Documentary decoding/scaling: `unsigned BE16 direct x/10; low-side x/1000`

- [Source 1](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/logs/csv/parameters_22.csv)

<a id="ignition"></a>

## Ignition, misfire, torque and airflow

**Signals · Unqualified candidate**

Applicability: Family evidence; exact current ECU/TCU unqualified

2004 average ignition and 200A–200D corrections are distinct. 2904 misfire count has an unresolved counting window. 437C/4380 torque is ECU-reported, not dyno wheel torque. 2032 air mass and 1070 pedal need independent semantics.

Units: degrees; count; Nm; kg/h; %

Documentary decoding/scaling: `signed BE16 ignition x/100, torque x/10; unsigned BE16 misfires x, airflow x, pedal x/10.24`

- [Source 1](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/logs/csv/parameters_22.csv)

<a id="gpf-questions"></a>

## GPF and DSG measurement gaps

**Open questions · Unqualified candidate**

Applicability: Family evidence; exact current ECU/TCU unqualified

Investigate explicitly labelled GPF differential pressure, soot versus ash estimates, regeneration state and temperature locations. No exact DIDs are established here; do not reuse diesel DPF definitions. DSG slip, clutch thermal state, pump state and selected/engaged gear need current TCU evidence.

Units: Unknown until individually qualified

Documentary decoding/scaling: `No invented GPF/TCU definitions`

- [Source 1](https://www.ross-tech.com/vcds/tour/controller-channels-map.php)
