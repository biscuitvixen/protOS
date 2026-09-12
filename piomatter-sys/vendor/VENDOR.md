# Vendored sources

Adafruit_Blinka_Raspberry_Pi5_Piomatter, tag 1.0.0, commit
5d46945596202982a9e3ab8357daa797903e2164 (released 2025-07-15).

- `piomatter/` is `src/include/piomatter/` upstream: the header-only
  HUB75 driver core. GPL-2.0-only (see ../LICENSE).
- `piolib/` is `src/piolib/` upstream: the RP1 PIO userspace library.
  These files carry a GPL-2.0 header at this tag; upstream
  raspberrypi/utils relicensed piolib to BSD-3-Clause on 2025-04-28
  (commit c57d8c29c46993d93f191218bbc1dc3a73fc7918).

Nothing here is modified. Update by replacing the directories from a
newer tag and recording the commit above.
