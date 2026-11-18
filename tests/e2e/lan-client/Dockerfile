FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends avahi-daemon avahi-utils dbus tcpdump iproute2 \
    && rm -rf /var/lib/apt/lists/*
COPY entrypoint.sh /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
