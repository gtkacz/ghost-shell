SHADER_DIR := shell/kit/GhostKit/shaders
SHADERS := $(wildcard $(SHADER_DIR)/*.frag)
COMPILED_SHADERS := $(SHADERS:.frag=.frag.qsb)
LAYOUT_TEST_CONFIG := /tmp/ghost-layout-test.toml

.PHONY: shaders release install install-term test smoke smoke-layout smoke-holds smoke-greeting

shaders: $(COMPILED_SHADERS)

%.frag.qsb: %.frag
	qsb-qt6 --qt6 -o $@ $<

release: shaders
	cargo build --release

install: release
	scripts/install.sh

install-term:
	scripts/install-term.sh

test:
	node --test 'shell/tests/*.test.mjs'
	cargo test --workspace

smoke: shaders
	scripts/qml-smoke.sh

$(LAYOUT_TEST_CONFIG): scripts/layout-test.toml
	cp $< $@

smoke-layout: shaders $(LAYOUT_TEST_CONFIG)
	GHOST_CONFIG=$(LAYOUT_TEST_CONFIG) scripts/qml-smoke.sh

smoke-holds: shaders
	scripts/smoke-holds.sh

smoke-greeting: shaders
	scripts/smoke-greeting.sh
