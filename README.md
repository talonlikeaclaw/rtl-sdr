# rtl-sdr

## sdr-compose-stack

Docker Compose profiles for radio apps on a single RTL-SDR Blog V4L (Lite) dongle on a Pi 4. One dongle means one SDR app at a time: each app lives behind its own compose profile, and only one profile is ever up.

## Usage

```bash
docker compose --profile adsb up -d      # start the ADS-B receiver
docker compose --profile adsb down       # tear it down
```

Switching apps = tear down the current profile, then up the next one's profile.

## Dongle driver

The dongle is an RTL-SDR Blog V4L. Its RF input switch is only configured by the
RTL-SDR Blog librtlsdr fork; the upstream image ships an older librtlsdr that
does not recognise `Blog V4L` and leaves the tuner on the wrong input (no 1090
MHz signal). `readsb` therefore builds `readsb-v4l/Dockerfile`, which installs
the blog fork over the image's library. Startup logs should show:

```
RTL-SDR Blog V4 Lite Detected
```

Rebuild after changing the Dockerfile with `docker compose --profile adsb build readsb`.

## Active services

| Profile | Services | What you get |
|---|---|---|
| `adsb` | readsb | ADS-B decoder; Beast output on host port 30005 |
| | tar1090 | Web map at `http://<pi-ip>:8080` |

## Roadmap

Each profile below gets added in its own phase, with a manual-test checkpoint before the next:

| Profile | App | Image | Band |
|---|---|---|---|
| `adsb` | readsb + tar1090 | `ghcr.io/sdr-enthusiasts/docker-readsb-protobuf`, `docker-tar1090` | 1090 MHz |
| `ais` | AIS-catcher | official Docker docs (jvde-github), explicit RTL-SDR V4 support | 162 MHz |
| `sat` | SatDump | official repo ships Dockerfile + docker-compose.yml | 137 MHz NOAA APT |
| `acars` | ACARSHub | `ghcr.io/sdr-enthusiasts/docker-acarshub` | 131 MHz (single dongle: ACARS first, VDL2 needs 2nd dongle) |
| `pocsag` | rtl_fm + multimon-ng → PagerMon | `pagermon/pagermon` official image | 150–160 MHz |
| `sondes` | radiosonde_auto_rx | official Docker wiki (projecthorus) | 400–406 MHz |
| `feeders` | ADS-B aggregators (ADSB.fi / ADSB Exchange) | TBD at that phase's planning | — |

Images and env vars for later phases get verified when each phase is planned; roadmap rows are intent, not yet runnable.
