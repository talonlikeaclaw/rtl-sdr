# adsb.fi feeder

Host-level feeder (adsb.fi has no Docker image). It reads the Beast stream that
`readsb` publishes on `30005` and sends reduced Beast to `feed.adsb.fi:30004`;
`mlat-client` sends timing data to `feed.adsb.fi:31090` and injects the returned
positions into `readsb` on `30104`.

## Why host-level

The upstream `adsb.fi/feed.sh` installer compiles a patched `readsb`
(`feed-adsbfi`) and `mlat-client`, creates a system user, and installs two
systemd units. Running it on the host is the supported path; the services
reconnect on their own when the `adsb` compose profile is down.

## Prerequisites

- `readsb` Beast output reachable at `127.0.0.1:30005` (published by compose).
- `readsb` Beast input reachable at `127.0.0.1:30104` for MLAT results:
  compose sets `READSB_NET_BEAST_INPUT_PORT=30104` and publishes
  `127.0.0.1:30104:30104`.
- Accurate antenna latitude, longitude, and **altitude above sea level**
  (MLAT needs ~15 m accuracy; ground elevation alone is not enough if the
  antenna is on a roof or mast).

## Install

```bash
sudo mkdir -p /usr/local/share/adsbfi
sudo git clone --depth 1 --single-branch --branch master \
  https://github.com/adsbfi/adsb-fi-scripts.git /usr/local/share/adsbfi/git
sudo cp feeders/adsbfi/adsbfi.env.example /etc/default/adsbfi
sudoedit /etc/default/adsbfi        # USER, LATITUDE, LONGITUDE, ALTITUDE
sudo bash /usr/local/share/adsbfi/git/update.sh
```

`USER="0"` disables MLAT and feeds ADS-B only. Set it to a unique public name
(e.g. `talon-montreal`) and the real antenna altitude to enable MLAT.

`USER` is published on the adsb.fi feeder network map
(<https://adsb.fi/network>; the map reads
<https://api.adsb.fi/v1/coverage>, whose `user` field is the feeder name).
Locations are rounded/offset for privacy. `PRIVACY="--privacy"` suppresses the
name and hides the feeder from that map.

## Verify

```bash
systemctl status adsbfi-feed adsbfi-mlat
sudo journalctl -u adsbfi-feed -n 20 --no-pager
sudo journalctl -u adsbfi-mlat -n 20 --no-pager
ss -tn | grep -E '30004|31090'      # outbound to feed.adsb.fi
```

`adsbfi-feed` logs `feed.adsb.fi:30004` connection state; `adsbfi-mlat` logs
`Server: ready` and a non-zero `peer_count` once enough receivers agree. MLAT
results appear in `tar1090` as aircraft with an MLAT position source.

The feeder-only snapshot endpoint returns `200` once adsb.fi has associated the
public IP with a feed:

```bash
curl -s -o /dev/null -w '%{http_code}\n' https://opendata.adsb.fi/api/v2/snapshot
```

## Config

`/etc/default/adsbfi` is owned by the installer. `adsbfi.env.example` mirrors
the fields; keep the `TARGET`, `NET_OPTIONS`, and `RESULTS` lines intact.

The UUID lives in `/usr/local/share/adsbfi/adsbfi-uuid` and identifies this
feeder to adsb.fi. Do not regenerate it unless you intend to appear as a new
feeder.

## Uninstall

```bash
sudo bash /usr/local/share/adsbfi/uninstall.sh
```
