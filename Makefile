APP_NAME := bibliotek
LABEL := com.gwuah.$(APP_NAME)
BIN_DIR := /usr/local/bin
CONFIG_DIR := $(HOME)/.config/$(APP_NAME)
LAUNCH_AGENTS_DIR := $(HOME)/Library/LaunchAgents
PLIST_NAME := $(LABEL).plist

.PHONY: build release install uninstall upgrade

build:
	cargo build

release:
	cd web && npm ci && npm run build
	cargo build --release

install: release
	cp target/release/$(APP_NAME) $(BIN_DIR)/
	mkdir -p $(CONFIG_DIR)
	@if [ ! -f $(CONFIG_DIR)/config.yaml ]; then \
		cp config.example.yaml $(CONFIG_DIR)/config.yaml; \
		echo "Created $(CONFIG_DIR)/config.yaml"; \
	else \
		echo "Config already exists, skipping"; \
	fi
	cp $(PLIST_NAME) $(LAUNCH_AGENTS_DIR)/
	@echo ""
	@echo "Next steps:"
	@echo "  1. Edit $(CONFIG_DIR)/config.yaml with your credentials"
	@echo "  2. Run: launchctl load $(LAUNCH_AGENTS_DIR)/$(PLIST_NAME)"

uninstall:
	-launchctl unload $(LAUNCH_AGENTS_DIR)/$(PLIST_NAME) 2>/dev/null
	rm -f $(LAUNCH_AGENTS_DIR)/$(PLIST_NAME)
	rm -f $(BIN_DIR)/$(APP_NAME)
	@echo "Config left at $(CONFIG_DIR)/ - remove manually if desired"

upgrade: release
	cp target/release/$(APP_NAME) $(BIN_DIR)/
	launchctl stop $(LABEL) && launchctl start $(LABEL)
	@echo "Upgraded and restarted $(APP_NAME)"
