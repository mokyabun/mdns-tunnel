#!/bin/sh
set -e
conf=/etc/avahi/avahi-daemon.conf
sed -i 's/^#*use-ipv6=.*/use-ipv6=no/; s/^rlimit-nproc=.*/#&/' "$conf"
if [ -n "$AVAHI_INTERFACES" ]; then
    sed -i "s/^#*allow-interfaces=.*/allow-interfaces=$AVAHI_INTERFACES/" "$conf"
fi
mkdir -p /run/dbus
rm -f /run/dbus/pid /run/avahi-daemon/pid
dbus-daemon --system
avahi-daemon --daemonize --no-chroot
exec sleep infinity
