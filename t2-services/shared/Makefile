PREFIX ?= /usr/local
LIBEXECDIR ?= $(PREFIX)/libexec
SYSTEMD_UNIT_DIR ?= $(PREFIX)/lib/systemd/system
SYSCONFDIR ?= /etc
INSTALL_NETWORK_PROFILE ?= yes

.PHONY: all install
all:
	@echo 'Shared transport is linked by consumers; integration needs no build.'
install:
	install -D -m 0755 ../../packaging/lifecycle/kait2en-lifecycle.py $(DESTDIR)$(LIBEXECDIR)/t2-services/lifecycle.py
	install -d $(DESTDIR)$(LIBEXECDIR)/t2-services/migration
	install -m 0644 integration/migration/* $(DESTDIR)$(LIBEXECDIR)/t2-services/migration/
	install -D -m 0755 integration/libexec/t2-ncm-sleep $(DESTDIR)$(LIBEXECDIR)/t2-services/t2-ncm-sleep
	install -D -m 0644 integration/libexec/package-actions $(DESTDIR)$(LIBEXECDIR)/t2-services/package-actions
	install -d $(DESTDIR)$(SYSTEMD_UNIT_DIR)
	sed 's|@LIBEXECDIR@|$(LIBEXECDIR)|g' integration/systemd/t2-services-suspend.service >$(DESTDIR)$(SYSTEMD_UNIT_DIR)/t2-services-suspend.service
	chmod 0644 $(DESTDIR)$(SYSTEMD_UNIT_DIR)/t2-services-suspend.service
	install -D -m 0644 integration/NetworkManager/10-t2-services.conf $(DESTDIR)$(SYSCONFDIR)/NetworkManager/conf.d/10-t2-services.conf
	install -d -m 0700 $(DESTDIR)$(SYSCONFDIR)/NetworkManager/system-connections
	if test "$(INSTALL_NETWORK_PROFILE)" = yes; then test -e $(DESTDIR)$(SYSCONFDIR)/NetworkManager/system-connections/t2-ncm.nmconnection || install -m 0600 integration/NetworkManager/t2-ncm.nmconnection $(DESTDIR)$(SYSCONFDIR)/NetworkManager/system-connections/t2-ncm.nmconnection; fi
