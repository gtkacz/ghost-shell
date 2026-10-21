SHADER_DIR := shell/kit/GhostKit/shaders
SHADERS := $(wildcard $(SHADER_DIR)/*.frag)
COMPILED_SHADERS := $(SHADERS:.frag=.frag.qsb)
LAYOUT_TEST_CONFIG := /tmp/ghost-layout-test.toml

.PHONY: shaders test smoke smoke-layout

shaders: $(COMPILED_SHADERS)

%.frag.qsb: %.frag
	qsb-qt6 --qt6 -o $@ $<

test:
	node --test 'shell/tests/*.test.mjs'
	cargo test --workspace

smoke: shaders
	scripts/qml-smoke.sh

$(LAYOUT_TEST_CONFIG): scripts/layout-test.toml
	cp $< $@

smoke-layout: shaders $(LAYOUT_TEST_CONFIG)
	GHOST_CONFIG=$(LAYOUT_TEST_CONFIG) scripts/qml-smoke.sh
