# readsb image + RTL-SDR Blog driver (V4/V4L aware).
#
# The upstream image ships librtlsdr 2.0.1, which predates RTL-SDR Blog V4L
# support. Without it the V4L's RF switch is never configured and the dongle
# receives essentially nothing. Build the rtl-sdr-blog fork and install it
# over /usr/local/lib/librtlsdr.so.0 (the SONAME readsb links against).
FROM ghcr.io/sdr-enthusiasts/docker-readsb-protobuf:latest

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      git cmake build-essential pkg-config libusb-1.0-0-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*

RUN git clone --depth 1 https://github.com/rtlsdrblog/rtl-sdr-blog /tmp/rtl-sdr \
 && cmake -S /tmp/rtl-sdr -B /tmp/rtl-sdr/build \
      -DINSTALL_UDEV_RULES=OFF -DDETACH_KERNEL_DRIVER=ON \
 && cmake --build /tmp/rtl-sdr/build -j"$(nproc)"

# Replace the old library and make sure the .so.0 symlink points at the new one.
RUN rm -f /usr/local/lib/librtlsdr.so /usr/local/lib/librtlsdr.so.0 \
          /usr/local/lib/librtlsdr.so.2.0.1 /usr/local/lib/librtlsdr.a \
 && cmake --install /tmp/rtl-sdr/build \
 && ln -sf "$(basename "$(ls -1 /usr/local/lib/librtlsdr.so.0.* | head -1)")" \
          /usr/local/lib/librtlsdr.so.0 \
 && ln -sf librtlsdr.so.0 /usr/local/lib/librtlsdr.so \
 && ldconfig \
 && rm -rf /tmp/rtl-sdr

RUN apt-get purge -y git cmake build-essential pkg-config libusb-1.0-0-dev \
 && apt-get install -y --no-install-recommends libusb-1.0-0 \
 && apt-get autoremove -y \
 && rm -rf /var/lib/apt/lists/*
