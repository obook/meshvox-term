# MeshCore Companion Radio protocol (Bluetooth)

Notes on the frames a MeshCore companion radio exchanges with its client
over Bluetooth Low Energy. meshvox-term does not build these frames itself
(the `meshcore-rs` crate in `vendor/` does), but they explain what the
crate sends, what the radio answers and why some commands fail.

The notes come from the MeshVox Android app (MIT, same author,
`docs/protocol-notes.md`). They were taken from the MeshCore firmware
source (MIT), commit `5a1b8d9`: `examples/companion_radio/MyMesh.cpp`
(the authority), `src/helpers/BaseChatMesh.{h,cpp}`,
`src/helpers/BaseSerialInterface.h`, `examples/simple_repeater/MyMesh.{h,cpp}`
and `docs/companion_protocol.md` (a draft, sometimes incomplete: the code
wins).

## General

- Multi-byte integers are little-endian. Text is UTF-8.
- Bluetooth: one frame is one write to `6E400002-...` (client to radio) or
  one notification on `6E400003-...` (radio to client). There is no length
  byte and no delimiter: the `<` / `>` and length bytes are only for USB
  and serial links.
- A frame is at most `MAX_FRAME_SIZE = 176` bytes. Ask for a large MTU
  (512): the default 20 useful bytes are not enough.
- One command at a time: wait for its reply (5 s is a good timeout).
- First byte of a frame from the radio: below `0x80` is the reply to a
  command, `0x80` and above is an unsolicited push.
- The protocol version sent with `DeviceQuery` (byte 1 = 3) makes the
  radio send messages in the V3 format, which carries the SNR.
- Scanning: the UART service `6E400001-...` is generic and not always
  advertised. Find the radio by its name, `MeshCore-...`, or by the
  service when it is there.

## Message length

- Direct text: `MAX_TEXT_LEN = 160` bytes of UTF-8 (10 x 16,
  `BaseChatMesh.h`). Beyond that the radio answers `Error(3)` (TABLE_FULL,
  which is misleading).
- Channel text: the radio puts `"<node name>: "` in front, and the whole
  is cut to 160 bytes without any error. The safe limit on the client side
  is 160 - (name bytes + 2).
- The limits are in UTF-8 bytes, not in characters.
- Received messages are cut to the frame size (176).

## Commands (client to radio)

| Command | Frame |
|---|---|
| AppStart | `01` + 7 reserved bytes (0) + app name in UTF-8 (optional). Length >= 8. Reply: SelfInfo. |
| DeviceQuery | `16 03` (protocol version 3). Reply: DeviceInfo. |
| GetBattery | `14`. Reply: Battery. |
| GetTime | `05`. Reply: DeviceTime. |
| SetTime | `06` + epoch u32. The radio refuses (`Error(6)`) a time earlier than its own. |
| SetName | `08` + name in UTF-8 (31 bytes at most, the radio cuts it). |
| SetCoords | `0E` + lat i32 + lon i32, in microdegrees (degrees x 1e6). Out of range: `Error(6)`. |
| SetTxPower | `0C` + dBm i8. Range on the radio: -9 to MAX_LORA_TX_POWER (read from SelfInfo). |
| SetRadioParams | `0B` + freq u32 (kHz) + bw u32 (**Hz**) + sf u8 + cr u8 [+ repeat u8]. Ranges on the radio: freq `150000..2500000` kHz, bw `7000..500000` Hz, sf 5 to 12, cr 5 to 8. |
| SendAdvert | `07` (zero hop) or `07 01` (flood). |
| GetContacts | `04` [+ since u32]. Replies: ContactsStart, N x Contact, ContactsEnd. |
| SyncNextMessage | `0A`. Reply: one message or NoMoreMessages. |
| SendDirect | `02` + txtType u8 (0 = text) + attempt u8 (0) + timestamp u32 + 6-byte key prefix + text. Length >= 14. Reply: Sent. |
| SendChannel | `03` + txtType u8 (0) + index u8 + timestamp u32 + text. Reply: Ok (no Sent, so no ack). |
| Login | `1A` + 32-byte public key + password in UTF-8 (empty = guest). Reply: Sent, then the push LoginSuccess or LoginFailed. |
| RequestStatus | `1B` + 32-byte public key. Reply: Sent, then the push StatusResponse. |
| RemoveContact | `0F` + 32-byte key. Ok, or Error(2) if unknown. |
| ResetPath | `0D` + 32-byte key. Ok, or Error(2). The path goes back to "unknown" (`0xFF`). |
| ShareContact | `10` + 32-byte key. Zero-hop advert of the contact. Ok, Error(2) unknown, Error(3) cannot send. |
| ExportContact | `11` [+ 32-byte key; no key = the radio itself]. Reply: ExportedContact, or Error(2). |
| ImportContact | `12` + advert packet. The frame must be longer than 98 bytes (the firmware tests `len > 2 + 32 + 64`). Ok, or Error(6) for a bad packet. |
| AddUpdateContact | `09` + key 32 + type u8 (1 to 4) + flags u8 + path length u8 (`0xFF` = unknown) + path 64 bytes (`MAX_PATH_SIZE`) + name 32 + last advert u32 [+ lat i32 + lon i32 [+ lastmod u32]]. The firmware reads the 64 path bytes and the 32 name bytes without a length check, so send the whole frame: 144 bytes, position (0, 0) meaning none. A known contact is OVERWRITTEN; a full table gives Error(3). |
| TracePath | `24` + tag u32 + auth u32 + flags u8 + path (bytes). Longer than 10 bytes. One hop id is `1 << (flags & 3)` bytes; the path length must be a multiple of it and at most 64 hops, otherwise Error(6). Reply: Sent (flood = 0, the request's tag, estimated delay in ms), then the push TraceData. |
| PathDiscovery | `34 00` + 32-byte key (the firmware's discovery request: flooded telemetry). Reply: Sent (flood = 1 when flooded, tag, estimated delay), Error(2) unknown contact, Error(3) cannot send; then the push PathDiscoveryResponse. |
| RequestTelemetry | `27` + 3 reserved bytes (0) + 32-byte key (36 bytes at least; a frame of exactly 4 bytes asks for the radio's OWN telemetry, without Sent). Reply: Sent (flood, tag, estimated delay), Error(2) unknown contact, Error(3) cannot send; then the push TelemetryResponse. |
| GetChannel | `1F` + index u8. Reply: ChannelInfo. |
| SetChannel | `20` + index u8 + name 32 bytes (padded with 0) + secret 16 bytes. Length >= 50. Reply: Ok, or Error(2) for an index out of range. |
| GetCustomVars | `28`. Reply: CustomVars. |
| SetCustomVar | `29` + `name:value` as text (length >= 4), for example `gps:0`. The radio applies and saves the setting (`gps` turns the GPS off or on). Reply: Ok, or Error(6) for an unknown setting. |

## Replies and pushes (radio to client)

| Code | Name | Content |
|---|---|---|
| 00 | Ok | - |
| 01 | Error | code u8 (1 unknown command, 2 not found, 3 table full, 4 bad state, 5 file I/O, 6 bad argument). |
| 02 | ContactsStart | total u32 |
| 03 | Contact | see below (148 bytes) |
| 04 | ContactsEnd | most recent lastmod u32 (reuse it as `since`) |
| 05 | SelfInfo | type u8, txPower i8, maxTxPower u8, public key 32, lat i32, lon i32, multiAcks u8, advertLocPolicy u8, telemetry u8, manualAdd u8, freq u32 (kHz), bw u32 (Hz), sf u8, cr u8, name (rest of the frame, not 0-terminated) |
| 06 | Sent | flood u8 (1 = flood), tag u32 (= the expected ack tag), estimated delay u32 (ms) |
| 07 | Direct message (V2) | prefix 6, pathLen u8 (`0xFF` = direct), txtType u8, timestamp u32, [signature 4 if txtType = 2], text |
| 08 | Channel message (V2) | index u8, pathLen u8, txtType u8, timestamp u32, text |
| 09 | DeviceTime | epoch u32 |
| 0A | NoMoreMessages | - |
| 0B | ExportedContact | advert packet (rest of the frame) |
| 0C | Battery | millivolts u16, storage used u32 (KB), total u32 (KB) |
| 0D | DeviceInfo | fwVer u8, maxContacts/2 u8, maxChannels u8, BLE PIN u32, date 12, maker 40, version 20, [repeat u8], [pathHashMode u8] (the last two are missing from old firmwares) |
| 10 | Direct message (V3) | snr i8 (x4), 2 reserved, then as 07 |
| 11 | Channel message (V3) | snr i8 (x4), 2 reserved, then as 08 |
| 12 | ChannelInfo | index u8, name 32 bytes (padded with 0), secret 16 bytes |
| 15 | CustomVars | `name:value` text separated by commas, for example `gps:1`; empty if the radio has no adjustable sensor. `gps` appears only when the firmware has a GPS. |
| 80 | Push Advert | public key 32 (a known contact was updated) |
| 82 | Push SendConfirmed | ack tag u32, round trip time u32 (ms) |
| 83 | Push MessagesWaiting | - (call SyncNextMessage in a loop until NoMoreMessages) |
| 85 | Push LoginSuccess | admin u8, prefix 6, [tag u32, perms u8, fwLevel u8] (8 bytes in all in the legacy reply) |
| 86 | Push LoginFailed | reserved u8, prefix 6 |
| 87 | Push StatusResponse | reserved u8, prefix 6, then RepeaterStats (56 bytes) |
| 89 | Push TraceData | reserved u8, path length u8 (in bytes), flags u8, tag u32, auth u32, hop ids (path length bytes), SNR of each hop (path length >> (flags & 3) bytes, int8 x 4), final SNR to this radio (int8 x 4) |
| 8A | Push NewAdvert | as Contact (148 bytes) |
| 8B | Push TelemetryResponse | reserved u8, 6-byte key prefix, then the telemetry in Cayenne LPP (rest of the frame) |
| 8D | Push PathDiscoveryResponse | reserved u8, 6-byte key prefix, outbound path length u8, outbound path, return path length u8, return path |

Contact (code 03 or 8A): public key 32, type u8 (1 chat, 2 repeater,
3 room, 4 sensor), flags u8, outPathLen u8 (`0xFF` = unknown), outPath 64,
name 32 (padded with 0), lastAdvert u32, lat i32 and lon i32
(microdegrees), lastmod u32. Position (0, 0) means no position.

RepeaterStats (56 bytes, in this order): battMv u16, txQueue u16,
noiseFloor i16, lastRssi i16, packetsRecv u32, packetsSent u32,
airTimeSecs u32, upTimeSecs u32, sentFlood u32, sentDirect u32,
recvFlood u32, recvDirect u32, errEvents u16, lastSnr i16 (x4),
directDups u16, floodDups u16, rxAirTimeSecs u32, recvErrors u32.

## Watch out

- **No radio message id** in the message frames: to drop duplicates, build
  a key from the sender or channel, the timestamp and the text.
- **Bandwidth** is in Hz in the frame: 62.5 kHz is `62500`.
- **Maximum power** is in SelfInfo (byte 3), not in DeviceInfo.
- **Acks**: only SendDirect gives an ack tag (`Sent.tag`); the push
  SendConfirmed carries the same tag. Channel messages have no ack.
- **Time**: SetTime is refused for a value earlier than the radio's clock.
- **Login and status**: the immediate reply is `Sent`; the result comes
  later as a push (`Sent` gives the estimated delay).
- The frame layouts come from reading the firmware code, not from a
  capture. Push Advert `0x80`: the content above (public key only) was
  read in the code, not seen on the air.

## Channels

From `MyMesh.cpp` (CMD_GET_CHANNEL = 31, CMD_SET_CHANNEL = 32,
RESP_CODE_CHANNEL_INFO = 18) and `BaseChatMesh.cpp` (`getChannel`,
`setChannel`).

- `GetChannel` answers `ChannelInfo` for EVERY index below
  `MAX_GROUP_CHANNELS` (40 on the ThinkNode M5: an empty slot has an empty
  name and a zero secret), and `Error(2)` only beyond. So read slots 0 to
  39, stop at the first `Error` and skip the empty ones.
- `SetChannel`: length >= 50; a frame with a 32-byte secret (length >= 66)
  gets `Error(1)` (not handled); an index out of range gets `Error(2)`. The
  radio saves the channels (`saveChannels`).
- The firmware cannot delete a channel.
- The name is copied over 32 bytes: 31 useful bytes at most.
- Public hashtag channel (`#test`): the secret is the SHA-256 of the name,
  `#` included, cut to 16 bytes, as in the official app and `meshcore_py`.
  Everyone who types the same name, with the same case, joins the same
  channel.

### Links and QR codes

Format of the official app (`MeshCore/docs/qr_codes.md`):
`meshcore://channel/add?name=<name>&secret=<32 hex characters>[&region_scope=<text>]`.
The name is URL-encoded (a space becomes `+` or `%20`). The link holds the
channel secret: never log it, and warn before showing it.

## Contacts

From `MyMesh.cpp` (CMD_ADD_UPDATE_CONTACT = 9, CMD_RESET_PATH = 13,
CMD_REMOVE_CONTACT = 15, CMD_SHARE_CONTACT = 16, CMD_EXPORT_CONTACT = 17,
CMD_IMPORT_CONTACT = 18, RESP_CODE_EXPORT_CONTACT = 11,
`updateContactFromFrame`, `MAX_PATH_SIZE = 64` in `MeshCore.h`).

- Adding a contact that is already there overwrites it without any check:
  refuse a known key on the client side.
- Share link:
  `meshcore://contact/add?name=<name>&public_key=<64 hex>&type=<1 to 4>`
  (1 companion, 2 repeater, 3 room, 4 sensor). The public key is no secret.

## Path tools

From `MyMesh.cpp` (CMD_SEND_TRACE_PATH = 36, CMD_SEND_PATH_DISCOVERY_REQ = 52,
PUSH_CODE_TRACE_DATA = 0x89, PUSH_CODE_PATH_DISCOVERY_RESPONSE = 0x8D,
`onTraceRecv`, `onContactPathRecv`) and `Packet.cpp` (`isValidPathLen`,
`writePath`).

- Path length byte (push `0x8D` and packets): the 6 low bits are the hop
  count, the 2 high bits are the size of one hop id minus 1. Size 4
  (value 3) is reserved and invalid. Path bytes = hops x size.
- Matching: TracePath carries a tag chosen by the client, sent back as is
  in the push `0x89` and in Sent; a trace reply with another tag is not
  ours. PathDiscovery: the radio keeps a single pending tag
  (`pending_discovery`) and the push `0x8D` has NO tag, so match it on the
  6-byte key prefix; a new discovery replaces the previous one.
- When the push never comes: the trace path is broken or the node is out
  of range, the node did not answer the discovery or the reply path is
  invalid (`isValidPathLen` false: the firmware sends nothing). MeshVox
  waits 30 s at most.
- The firmware ignores the telemetry received with a discovery.
- The firmware cannot tell two traces apart if they run at the same time:
  run one at a time.

## Reading a repeater remotely

From `companion_radio/MyMesh.cpp` (CMD_SEND_STATUS_REQ = 27,
CMD_SEND_TELEMETRY_REQ = 39, PUSH_CODE_TELEMETRY_RESPONSE = 0x8B,
`onContactResponse`, `clearPendingReqs`), `simple_repeater/MyMesh.cpp` and
`simple_room_server/MyMesh.cpp` (answers to REQ_TYPE_GET_STATUS = 1 and
REQ_TYPE_GET_TELEMETRY_DATA = 3).

- Sequence: Login (Sent, then the push LoginSuccess or LoginFailed),
  RequestStatus (Sent, then the push `0x87`), RequestTelemetry (Sent, then
  the push `0x8B`).
- Matching: the radio keeps only ONE pending request at a time
  (`clearPendingReqs` at each new request), so wait for each reply before
  sending the next request. The radio matches the login on the first 4 key
  bytes, the status the same way (legacy scheme) and the telemetry on the
  request's tag (internal to the radio). The pushes `0x87` and `0x8B`
  carry the 6-byte key prefix: match on that. LoginSuccess and LoginFailed
  carry a prefix too, but with one login at a time it is not needed.
- When nothing comes: the node is out of range, the request was lost, or
  it was replaced by a newer one in the meantime. MeshVox waits 30 s at
  most in all.
- Repeaters AND rooms (types 2 and 3) answer both requests; the sensor
  example (`simple_sensor`) handles neither.
- A guest (empty password) gets only the basic telemetry (battery voltage
  and processor temperature); the other sensors need higher rights
  (`PERM_ACL_GUEST`).
- Telemetry is Cayenne LPP (ElectronicCats library 1.6.1, not in the
  firmware tree: the byte order is the one of the Cayenne LPP
  specification, big-endian, NOT checked in the source). Record: channel
  u8, type u8, data. Useful types: 116 voltage (2 bytes, 0.01 V),
  103 temperature (2 signed bytes, 0.1 degree), 104 humidity (1 byte,
  0.5%), 115 pressure (2 bytes, 0.1 hPa), 136 GPS (3 signed bytes of
  latitude and 3 of longitude, 0.0001 degree, then 3 bytes of altitude,
  0.01 m), 2 analog input (2 signed bytes, 0.01). Any other type has an
  unknown size: stop decoding there and drop the rest; a truncated payload
  stops decoding too.
