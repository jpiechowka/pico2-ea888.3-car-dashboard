# Diagnostic transport contracts

This stage adds shared pure modules exercised on the host and compiled for
RP2350. They are not yet connected to UART. No captured DNUC ECU reply or physical
CAN behavior has been validated.

## Wire and ownership boundaries

`CanFrame` represents a standard data frame with a known DLC of zero through
eight. Its fields are private and construction checks both ID and length.
Longan transmission requires exactly eight data bytes and emits TX14 with a
big-endian ID and zero EXT/RTR flags.

`Rx12` separately produces `RawRecord`. It deliberately provides no implicit
conversion to `CanFrame`: the receive wire carries neither DLC nor EXT/RTR, and
the published firmware serializes an unspecified tail after a short CAN frame.
All byte splits preserve a record only after a confirmed configuration boundary.
A known error discards the partial record, changes epoch and refuses further
bytes until another qualified transition. Invalid standard IDs also close the
stream. Neither silence nor a plausible ID can reestablish alignment. Undetected
insertion/deletion/substitution and whole-record loss remain protocol limitations.

The caller must qualify the exact bridge firmware's setting/data-mode banners,
consume the entire `ENTER DATA MODE` transition and retain following bytes from
that RX chunk. The future hardware owner must separately qualify standard DLC8
traffic or retain values as unavailable. A configuration ACK cannot prove filter
register readback; this firmware exposes no such readback command.

## ISO-TP and read services

The normal-addressed classical CAN receiver is bounded to 256 response bytes.
It validates SF/FF/CF shape against known DLC, checks the selected responder and
epoch, requires explicit flow-control completion, checks the four-bit sequence
including wrap, and aborts on malformed data or time regression. Replies beyond
capacity emit overflow flow control. No partial response is returned as complete.
CAN FD and extended-length forms are deliberately unsupported.

CTS requests a block size of four and STmin of 5 ms. N_Cr is 1000 ms; the
caller-provided total deadline is a separate application policy. These selected
flow-control settings have not been measured on this ECU. The receiver does
not infer actual CAN spacing from UART delivery timestamps. A missing FC completion
cannot admit CF bytes; transport TX completion will still be only UART completion
because this Longan firmware provides no CAN send result.

The request type exposes only OBD Mode01 and UDS ReadDataByIdentifier. Replies
must echo the requested service/PID/DID; negative responses retain their NRC,
and NRC78 is identified as pending. The next owner/scheduler slice must bound
pending extensions and correlate these events with one outstanding request.
No session change, security access, actuator control or write API is exposed.
No manufacturer DID is enabled by these raw read/response facilities.

Generic decoding currently includes PID05 coolant, PID0F intake, PID5C engine oil,
PID42 control-module voltage, and capability pages 00/20/40. Exact data widths are
required. Temperature arithmetic widens before subtracting 40; voltage uses BE16
millivolts without integer truncation. Capabilities belong to one ECU/epoch,
require advertised continuation pages, reject contradictory page rewrites and
map page-edge PIDs through the previous page. A missing response means unknown,
not unsupported. An explicit clear continuation bit excludes later pages.

## Three-check source record

| Contract | References checked |
| --- | --- |
| Longan TX14/RX12, commands and missing metadata | [Official documentation](https://docs.longan-labs.cc/1030001/), [official Arduino library](https://github.com/Longan-Labs/Serial_CAN_Arduino/blob/ffeaaa20b10d84771800b9407395f000dc8d4f1a/src/Serial_CAN_Module.cpp), [official bridge firmware](https://github.com/Longan-Labs/Serial_CAN_Bus/blob/c11ae43a81e9f9fc87fae56f70c6a14ccff53ce9/Serial_CAN_Firmware/cmdMode.cpp) |
| ISO-TP types, length, sequence and flow control | [Linux implementation](https://github.com/torvalds/linux/blob/a5ebb76233b79db01e82e061173cb73c6d2b5c6b/net/can/isotp.c), [Zephyr implementation](https://github.com/zephyrproject-rtos/zephyr/blob/920074c07cf4b503f70670cf424e4add4325d05a/subsys/canbus/isotp/isotp.c), [ISO15765-2:2016 original mirror](https://raw.githubusercontent.com/devcoons/iso15765-canbus/master/doc/ISO-15765-2-2016.pdf); also [ELM327 datasheet](https://www.elmelectronics.com/wp-content/uploads/2017/01/ELM327DS.pdf) |
| PID05/0F/42/5C and supported bits | [python-OBD](https://github.com/brendan-w/python-OBD/tree/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd), [ELMduino](https://github.com/PowerBroker2/ELMduino/blob/e9d6b55fd63f4afd7789d0981861e9990cdfcd8e/src/ELMduino.cpp), [ArduinoOBD](https://github.com/stanleyhuangyc/ArduinoOBD/blob/ed4dcb923153c2dcab2b88cab818c29c7f0820eb/libraries/OBD/OBD.cpp) |

These implementations corroborate the four basic formulas; their parser policies
are not copied. ELMduino's page-boundary helper and ArduinoOBD's integer voltage
normalization have limitations avoided here. Three checks do not mean every
upstream library agrees on every other PID.

MAP and wideband lambda remain unqualified. PID4F can override their scaling and
must be established per ECU/epoch. Current [CSS manufacturer DBC v4.3](https://canlogger.csselectronics.com/files/dbc-files/obd-dbc-files-v4.3.zip)
uses AB/32768 for default lambda; older SAE tables use 2*AB/65535. Some general OBD
libraries expose only the accompanying voltage/current or misroute wideband
through a narrowband decoder. These disagreements are preserved as evidence
limits. Neither PID44 commanded lambda nor catalyst PID3C is substituted for a
measured lambda or a qualified physical EGT sensor.

## Verification

Host fixtures cover every payload length 1..256, sequence wrap/block boundaries,
all record splits, fault at every byte, malformed/truncated/oversized frames,
wrong source/epoch, clock regression, deadline boundaries and missing FC. All
256 temperature inputs and 65536 voltage inputs are checked, with all 32 bits
of each capability page and source/epoch isolation. These are deterministic
software contracts, not vehicle replay captures. Live scheduling, UART recovery,
bridge qualification and physical validation remain separate gates.

An independent review reproduced an initially too-short N_Cr policy; the corrected
fixture now accepts a frame at 150 and 1000 ms and rejects 1001 ms while keeping
the total deadline at 5000 ms. No remaining blocking defect was identified in the
reviewed subset. Padding bytes are permissive, and invalid frames abort the
request; this is not a full ISO profile conformance claim.

125 runtime tests plus two compile-fail ownership checks pass in both outline
modes, with strict host/firmware lints and the stock target release/ELF gate.
The pure modules are not yet instantiated on the target, so linked static RAM
remains 374352 B with 149936 B headroom. The 30-sample host baseline in
`host/baselines/stage06_protocol` preserves source hashes and raw measurements:

| Fixture | Local median |
| --- | ---: |
| One synchronized RX12 record | 27.3 ns |
| Known-DLC ISO-TP single frame plus OBD voltage decode | 33.6 ns |
| 128-byte ISO-TP reassembly with block flow-control acknowledgements | 366.5 ns |

These are new parser CPU baselines; no UART waits, CAN traffic or physical
latency is measured. Adjacent host runs varied by several percent even in the
unchanged RX12 path, so no gain is attributed to the deadline correction.
