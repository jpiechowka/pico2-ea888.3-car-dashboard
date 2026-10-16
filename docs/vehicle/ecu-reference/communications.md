# Communications and qualification

This is a reference and a future capture plan, not an acquisition workflow or proof of live support. No vehicle, adapter or hardware was connected for this feature.

## Layers and read-only limits

Read-only diagnostics still transmit requests and ISO-TP flow control. They are not passive CAN listening. The [Linux ISO-TP documentation](https://www.kernel.org/doc/html/latest/networking/iso15765-2.html) explains single/first/consecutive/flow-control frames and configurable addressing, padding, block size and separation timing.

`01 PID` is an OBD Mode 01 request; `22 DID_hi DID_lo` is a UDS ReadDataByIdentifier request. Positive replies echo `41 PID` or `62 DID_hi DID_lo`. Do not prepend the `22` service twice when copying a tool's display identifier. Complete reassembly, responder/session identity, exact payload width, byte order, signedness and semantic validation must all precede use of a value. An echoed identifier alone is insufficient.

[Pinned VW_Flash](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/lib/simos_hsl.py) and [SimosTools](https://github.com/Switchleg1/SimosTools/blob/f99771ceb0442c863601a84a282c6e67e6b311a8/app/src/main/java/com/app/simostools/Constants.kt) use family candidate engine routes 7E0/7E8 and the latter a DSG route 7E1/7E9. The historical screenshot does not qualify these addresses, bitrate, frame format, padding or bridge behavior on this deployment. Logger payload parsers rely on transport support and cannot replace raw CAN reassembly.

Negative reply `7F requested_service NRC` retains its original result. NRC 78 means pending, not a sample. A timeout does not establish unsupported. Keep service/identifier denial, transient conditions, malformed replies and transport loss distinct; do not automatically change sessions or unlock security. [udsoncan timing documentation](https://udsoncan.readthedocs.io/en/latest/udsoncan/client.html) distinguishes initial-response P2, pending-response P2* and total deadlines. Host limits are not measured ECU timing guarantees.

Custom `3E32`/`3E33` logger setup/read commands are not ordinary TesterPresent merely because the first byte matches. RAM addresses, dynamic definitions and patches require exact software parity and are outside this dashboard's present read-only request constructors. [VehiCAL's basic/advanced comparison](https://docs.vehical.net/logger_basicadvanced) explains why predefined identifiers and RAM logging have different requirements.

## Identity and support discovery

Preserve current engine and TCU identities separately, including software/tune provenance, session and timestamp. [VW_Flash's identity list](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/lib/constants.py) supplies family candidates F19E/F1A2/F187/F189/F191/F1A3/F197/F1AD; their exact widths and semantics still need validation on this ECU. VIN is unnecessary for a public reference and should not be collected merely to populate one.

Mode 01 capability pages establish support for the selected responder and current identity/session epoch. An unqueried state, timeout and established unsupported state are different. For dependent wideband/MAP candidates, preserve all four PID 4F bytes and qualify their state before interpreting samples. Each zero field selects its own original definition; it does not cancel other nonzero fields. BARO remains independent. Requalify after identity/session changes.

The [historical SAE J1979 MAY2007 tables](https://www.autonerdz.com/yabbfiles/Attachments/1979v3.pdf), printed pages 122, 129, 132–133, 139 and 142–144, and [J1979-DA OCT2011 copy](https://www.e90post.com/forums/attachment.php?attachmentid=1509324&d=1476497126), pages 13, 20, 23–24, 30 and 34–35, preserve conflicting default ratio/voltage constants. Their printed `2/65535` / `8/65535` differ from zero-default prose `1.999/65535` / `7.999/65535`; pinned implementations also use `1/32768` for ratio. These historical copies do not determine the applicable current annex. The catalogue keeps these differences visible and enables no decoder.

## Future qualification record

Keep four concepts separate: **evidence** (what a source says), **applicability** (which controller it describes), **implementation** (whether executable decoding exists), and **live qualification** (current physical evidence).

1. Record current engine or TCU identity, session, operating state, responder and acquisition time.
2. Export measuring values with exact labels, locations and units. [Ross-Tech channel maps](https://www.ross-tech.com/vcds/tour/controller-channels-map.php) and [OBDeleven live data](https://support.obdeleven.com/en/articles/11009883-what-is-live-data) are source-discovery aids; labels and IDE names are not a conversion into wire DIDs.
3. Capture exact read requests/replies, payload width, endian/signedness, scale, sentinel and negative-response behavior. Preserve both measured and commanded signals independently.
4. Compare under appropriate key-on/engine-off, idle and warming conditions with an independent reference. Record measured versus modelled and sensor location explicitly.
5. Establish latency, sample age, skew, restart/disconnect behavior and aggregate polling budget through the actual bridge before batching or increasing rates.
6. Retain sanitized replay fixtures and admit a supported profile only after explicit review. A reference card or config edit cannot grant qualification.

Pause dashboard polling before connecting another diagnostic tool; [CANchecked's setup guide](https://www.canchecked.de/downloads/uds-einrichtung/) notes competing-tool connection issues. No coding, adaptations, forced regeneration, clearing or flashing is needed for this reference. Current generic coolant/intake/oil/module-voltage implementation remains unchanged; all speculative sources remain unavailable.
