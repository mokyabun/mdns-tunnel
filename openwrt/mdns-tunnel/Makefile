include $(TOPDIR)/rules.mk

PKG_NAME:=mdns-tunnel
# The release build passes PKG_VERSION from the git tag.
PKG_VERSION:=0.1.0
PKG_RELEASE:=1

PKG_MAINTAINER:=mokyabun <mokyabun@gmail.com>
PKG_LICENSE:=MIT
PKG_LICENSE_FILES:=LICENSE

# Prebuilt static musl binary from `docker build --target export --output dist .`;
# CURDIR resolves a symlinked package directory to its path in the repository.
MDNS_TUNNEL_BIN ?= $(CURDIR)/../../dist/mdns-tunnel

include $(INCLUDE_DIR)/package.mk

define Package/mdns-tunnel
  SECTION:=net
  CATEGORY:=Network
  TITLE:=mDNS relay over a TCP tunnel
  URL:=https://github.com/mokyabun/mdns-tunnel
endef

define Package/mdns-tunnel/description
  Relays mDNS packets of selected services between the LAN and
  a remote host over a TCP connection inside WireGuard, so LAN
  devices can discover services such as HomeKit Bridge.
endef

define Package/mdns-tunnel/conffiles
/etc/config/mdns-tunnel
endef

define Build/Compile
endef

define Package/mdns-tunnel/install
	$(INSTALL_DIR) $(1)/usr/sbin $(1)/etc/init.d $(1)/etc/config $(1)/etc/uci-defaults
	$(INSTALL_BIN) $(MDNS_TUNNEL_BIN) $(1)/usr/sbin/mdns-tunnel
	$(INSTALL_BIN) ./files/mdns-tunnel.init $(1)/etc/init.d/mdns-tunnel
	$(INSTALL_CONF) ./files/mdns-tunnel.config $(1)/etc/config/mdns-tunnel
	$(INSTALL_BIN) ./files/mdns-tunnel.uci-defaults $(1)/etc/uci-defaults/90-mdns-tunnel
endef

define Package/mdns-tunnel/postrm
#!/bin/sh
[ -n "$${IPKG_INSTROOT}" ] && exit 0
uci -q delete firewall.mdns_tunnel || exit 0
uci commit firewall
/etc/init.d/firewall reload >/dev/null 2>&1
exit 0
endef

$(eval $(call BuildPackage,mdns-tunnel))
