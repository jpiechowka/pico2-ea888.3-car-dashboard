# CANChecked U-DNU profile evidence

Reviewed 11 October 2026. The user supplied a local `U-DNU.TRX` alongside
the audit bundle. Keep that vendor file and all audit inputs out of Git.
The shared Drive `UDS OBD-Port` folder visibly lists a file with the same
name, size 15 KB and modification date 20 January 2025, but its preview is
unavailable and Drive returned an HTTP error on download. A byte-for-byte
identity between the local file and Drive copy has therefore not been proved.

The local file is **15,591 bytes**, SHA-256
`0e3e4f646d349b6266a27eaaa494dea6e3a905a3deafda495dc609fea7372947`.
It begins with the ASCII header `info;1.0;0;0;0;0;0;0;0;0;2;1;-notes-;`
followed by binary content. Printable-string inspection does not yield a
trustworthy request ID, response ID, DID, unit or scaling table. Random
byte patterns in this binary must not be interpreted as protocol fields.

[CANChecked's UDS setup guide](https://www.canchecked.de/downloads/uds-einrichtung/)
says its TRI filename uses `UDS` plus the first three letters of the engine
code. Its [Gen2 manual](https://www.canchecked.de/updater/MFD2832-Gen2-manual.pdf)
describes `U-` TRI/TRX files for UDS firmware and selecting the appropriate
engine-code file. Its [Gen2 download page](https://www.canchecked.de/downloads/downloads-mfd28-mfd32-mfd32s-mfa28-gen2/)
lists UDS firmware for VAG cars including Seat Leon 5F. These vendor statements
support treating `U-DNU` as a **candidate DNU-family UDS gauge profile**.
They do not establish that this exact binary supports the user's DNUC
Simos 18.10 ECU software, or reveal its sensor semantics.

The 2019 DNUC target still needs a saved ECU/TCU identification, timestamped
raw request/response captures for each claimed value, unit/scaling checks,
negative-response and timeout behavior, and an independent scan-tool
comparison on the actual vehicle. Until then the dashboard must show those
metrics as unverified/unavailable in Vehicle mode. This file is a research
lead, not a production DID source.
