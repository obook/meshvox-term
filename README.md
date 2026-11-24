# MeshVox Term

A terminal client for a MeshCore or Meshtastic radio over Bluetooth, or a
MeshCore radio plugged in over USB. You can send and receive messages,
direct or on the public channel, and see the nodes on a map. The program finds out which firmware the radio runs.

Tested on Linux (BlueZ) with an ELECROW ThinkNode M5 running either the
MeshCore Companion Bluetooth v1.18.0 firmware from
https://meshcore.io/flasher or Meshtastic 2.7.26.

The two looks, Normal and Hacker (sample data, the map tiles are not shown):

<p>
  <img src="media/normal.png" alt="Normal look" width="49%">
  <img src="media/hacker.png" alt="Hacker look" width="49%">
</p>

## Build

You need Rust.

```
cargo build --release
```

The program is `target/release/meshvox-term`.

## Pair the radio

The radio wants a bond made with its PIN; pairing from the KDE Bluetooth
settings leaves a bond the radio drops a few seconds after each connection.
Pair from meshvox-term instead: in the Device tab, press `p`, then type the
PIN shown on the radio's screen (the M5 picks a new one at each boot). It
removes the old bond first, and does it on its own once when a Meshtastic
radio's bond is stale. Or pair once from `bluetoothctl`:

```
bluetoothctl
agent KeyboardOnly
default-agent
scan on
pair AA:BB:CC:DD:EE:FF
```

Put your radio's address in the last line, then type the PIN. Or let
`scripts/pair-radio.py` do it all: it finds the radio, removes an old
pairing and asks for the PIN when bluetoothctl needs it. After switching the
radio to the other firmware, pair again: the old bond no longer works. To check the link:

```
meshvox-term --probe
```

## Run

```
meshvox-term
```

A menu asks for the look, Normal or Hacker. The program then connects to
the first radio it finds, MeshCore (named "MeshCore-...") or Meshtastic
(found by its Bluetooth service), and reconnects on its own. The header
names the firmware.

With Meshtastic:

- channel 0 is the public channel; direct messages get a receipt when the
  destination acknowledges them;
- nodes come from the radio's node database, without a contact list;
- `p` traces the route to any node (traceroute) instead of asking a
  repeater for its status;
- the program sets the radio's clock at each connection;
- the conversations of the two firmwares are kept apart in the history.

Options:

- `--device <name or address>`: the radio to use
- `--serial <port>`: the USB serial port of a MeshCore radio, see
  [Over USB](#over-usb)
- `--probe`: print the radio's state and exit
- `--cli "<command>"`: send a text command to a MeshCore radio, print its
  reply and exit, e.g. `--cli "set ui.backlight_on_keys 1"`
- `--image auto|sixel|kitty|iterm2|halfblocks`: how the map image is shown
- `--tiles ign|osm`: the map, Plan IGN (the default, with OpenStreetMap
  outside France) or OpenStreetMap
- `--console`: ASCII look with a radar of letters instead of the map;
  automatic on the Linux console (`TERM=linux`)

The map looks best in a terminal that shows images with the Kitty, Sixel
or iTerm2 protocol, like Konsole. Other terminals get a coarser picture
made of half blocks.

## Over USB

MeshCore only, tested on a ThinkNode M1 with the Companion USB v1.18.0
firmware. Meshtastic over USB is not supported.

1. Flash the MeshCore "Companion USB" firmware from
   https://meshcore.io/flasher: the Companion Bluetooth firmware does not
   talk over USB.
2. Give your user the right to open serial ports, then log out and in
   again:

   ```
   sudo usermod -aG dialout $USER
   ```

   That group is `dialout` on Debian, Ubuntu and Fedora, `uucp` on Arch.
   `ls -l /dev/ttyACM0` shows the group of the port.

3. Plug the radio in. It shows up under `/dev/serial/by-id/`, as
   `/dev/ttyACM0` for instance:

   ```
   ls -l /dev/serial/by-id/
   ```

4. Run `meshvox-term`, or `meshvox-term --probe` to check the link.

Without `--device` or `--serial`, the program asks each port of
`/dev/serial/by-id` whether a MeshCore radio answers, within 2 seconds,
and falls back to the Bluetooth scan when none does; it looks at the ports
again before each Bluetooth attempt, so a radio plugged in later is found.
A port that opens but stays silent (another device) is not asked again
while it stays plugged in. A port the user cannot open is named in the
banner, with the group to join.

`--serial /dev/ttyACM0` names the port and skips the search and Bluetooth.
Use it when the radio resets as its port opens (a radio behind a USB-UART
bridge), since the search then finds it silent. `--probe` and `--cli`
follow the same rules. `--device` and `--serial` cannot be given together.

Once on a serial port, the program stays on it until it is restarted: the
header names the port, and after an unplug or a reboot (`r` in the Device
tab) it opens the same port again every 5 seconds. There is no pairing over
USB: `p` in the Device tab answers "no pairing over USB". Everything else
works as over Bluetooth: messages, contacts, map and the radio settings.

## Tabs and keys

The screen has four tabs, as in the MeshVox Android app:

- **1 Chats**: the public channel and the conversations, latest first, with
  their last message (a MeshCore channel under the name the radio gives it,
  and you can write in it); the open conversation on the right
- **2 Contacts**: the nodes, with the selected node's card. Four filters,
  as in MeshVox: Contacts (people), Favorites, Repeaters, All (with my
  radio), Contacts first; and the online, offline and total counts. The
  Map tab's card and its `[` and `]` keys follow the filtered list: pick
  Repeaters to ping a repeater. The favorites are the
  ones set on the radio, from the MeshVox app for instance
- **3 Map**: the map and the selected node's card
- **4 Device**: my radio's card, and the console that changes its settings

A node heard in the last hour is online: its name is in bold wherever it
is written, and marked in the contact list.

Keys:

- `1` to `4`: open a tab
- Up and Down: pick a conversation (Chats) or a node (Contacts)
- Left and Right: pick a filter (Contacts)
- Enter: write to the open conversation (Chats) or to the selected node
  (Contacts); in the input line, send
- Esc or Tab: leave the input line
- PgUp and PgDn: scroll the conversation
- Arrows: pan the map; `+` and `-`: zoom; `[` and `]`: previous or next
  node (Map)
- `p`: ping the selected node (MeshCore repeater or room), or trace the
  route to it (Meshtastic), from the Contacts and Map tabs
- `s` in the Map tab, Hacker look: turn the radar sweep on or off
- `p` in the Device tab: pair the radio again; type its PIN, then Enter
  (Esc cancels)
- `:` in the Device tab: the console, under the radio's card. Type a
  command, then Enter; the radio's answer (OK, or ERR and why) follows it.
  `help` lists the commands; Up and Down bring back the last ones, PgUp and
  PgDn scroll, Esc closes. Settings of a MeshCore radio: `set name bbok`,
  `set tx 20`, `set radio 869.618 62.5 8 8` (MHz kHz SF CR) or `set radio
  preset 10` (`presets` lists them), `set pos 44.8378 -0.5792`, `set region
  fr-33`, `set hash 2`, `set gps on`, `set share off` (my position in my
  adverts), `set telemetry on`; `get` shows them. Also `advert`, `advert
  flood`, `map upload` (publish on map.meshcore.io), `reboot`, `pair`
- `a` and `f` in the Device tab: advert to the neighbours, or to all
- `r` in the Device tab: reboot the radio (MeshCore); the link comes back
  on its own, and a new node name reaches Bluetooth
- `q` or Ctrl+C: quit

Outside the input line only, the digits switch tabs and `q` quits.

## Files

- Messages: `~/.local/share/meshvox-term/history.jsonl`
- Map tiles: `~/.cache/meshvox-term/tiles-ign/` (Plan IGN) and
  `~/.cache/meshvox-term/tiles/` (OpenStreetMap)
- Log: `$XDG_RUNTIME_DIR/meshvox-term.log`

The project was called loraterm, and meshcore-m5 before that. On its first
start, meshvox-term moves the old `~/.local/share/` and `~/.cache/` folders
of those names to the new ones, unless the new ones already exist.

The plan is in [ROADMAP.md](ROADMAP.md).

## License

GPL-3.0, see [LICENSE](LICENSE).

- Map tiles: (c) IGN (Plan IGN, Licence Ouverte, through the Geoplateforme)
  and (c) OpenStreetMap contributors.
- Node icons: map.meshcore.io by Rastislav Vysoky, MIT, see
  `assets/node_types/LICENSE`.
- `vendor/meshcore-rs`: meshcore-rs 0.2.0 (MIT) with our Bluetooth fixes.
