CARGO ?= cargo
INSTALL ?= install
BIN = target/release/t2-smc-control

.PHONY: build install clean

build:
	$(CARGO) build --release

install: build
	$(INSTALL) -Dm755 $(BIN) /usr/local/bin/t2-smc-control
	$(INSTALL) -Dm644 org.t2smccontrol.gtk.desktop /usr/local/share/applications/org.t2smccontrol.gtk.desktop

clean:
	$(CARGO) clean
