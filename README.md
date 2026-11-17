# rtl-sdr

ADS-B and friends on a Pi 4 with one RTL-SDR Blog V4L dongle. Docker Compose
profiles, one radio app at a time (single dongle, single tuner).

## Run it

```bash
docker compose --profile adsb up -d     # start ADS-B
docker compose --profile adsb down      # stop
```

Map: `http://<pi-ip>:8080` — feed stats on port 30005 (Beast).
Switching apps = down the current profile, up the next one's.

## The dongle quirk (read this before debugging "no planes")

The V4L's RF input switch is only set by the RTL-SDR Blog librtlsdr fork.
The stock readsb image ships an older librtlsdr that doesn't know the V4L,
so the tuner sits on the wrong input and 1090 MHz looks dead. `readsb-v4l/`
builds the fork over the image's library. If the map is empty, check
`docker logs readsb` for:

```
RTL-SDR Blog V4 Lite Detected
```

Rebuild after touching the Dockerfile: `docker compose --profile adsb build readsb`.

## What's running

| Profile | Services | Ports |
|---|---|---|
| `adsb` | readsb, tar1090, plane-stats (see AGENTS.md) | 8080 (map), 30005 (Beast), 8081 (stats), 30104 (MLAT results, localhost) |

`adsb.fi` is fed by host systemd services (`adsbfi-feed`, `adsbfi-mlat`), not a
compose profile. They read the same Beast stream and retry while `adsb` is down;
see `feeders/adsbfi/`.

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
| `feeders` | adsb.fi done (host systemd, `feeders/adsbfi/`); ADSB Exchange/others TBD | — | — |

Images and env vars for later phases get verified when each phase is planned; roadmap rows are intent, not yet runnable.
