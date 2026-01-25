.PHONY: build release install uninstall

build:
	cargo build

release:
	cd web && npm ci && npm run build
	cargo build --release

install: release
	cp target/release/bibliotek /usr/local/bin/
	mkdir -p ~/.config/bibliotek
	@if [ ! -f ~/.config/bibliotek/config.yaml ]; then \
		cp config.yaml ~/.config/bibliotek/; \
	else \
		echo "Config already exists at ~/.config/bibliotek/config.yaml, skipping"; \
	fi
	cp com.gwuah.bibliotek.plist ~/Library/LaunchAgents/
	@echo "Edit ~/Library/LaunchAgents/com.gwuah.bibliotek.plist to set AWS credentials"
	@echo "Then run: launchctl load ~/Library/LaunchAgents/com.gwuah.bibliotek.plist"

uninstall:
	-launchctl unload ~/Library/LaunchAgents/com.gwuah.bibliotek.plist 2>/dev/null
	rm -f ~/Library/LaunchAgents/com.gwuah.bibliotek.plist
	rm -f /usr/local/bin/bibliotek
	@echo "Config left at ~/.config/bibliotek/ - remove manually if desired"
