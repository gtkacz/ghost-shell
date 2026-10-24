SHADER_DIR := shell/kit/GhostKit/shaders
SHADERS := $(wildcard $(SHADER_DIR)/*.frag)
COMPILED_SHADERS := $(SHADERS:.frag=.frag.qsb)
LAYOUT_TEST_CONFIG := /tmp/ghost-layout-test.toml

.PHONY: shaders release install install-term test test-gate gate-shots smoke smoke-layout smoke-holds smoke-greeting smoke-lock smoke-login

shaders: $(COMPILED_SHADERS)

%.frag.qsb: %.frag
	qsb-qt6 --qt6 -o $@ $<

release: shaders
	cargo build --release

install: release
	scripts/install.sh

install-term:
	scripts/install-term.sh

test: test-gate
	node --test 'shell/tests/*.test.mjs'
	cargo test --workspace

# The software renderer that plain offscreen picks draws no ShaderEffect, so dots, stripes and the blink would be missing.
test-gate: shaders
	tmp=$$(mktemp -d) && trap 'rm -rf "$$tmp"' EXIT && scripts/assemble-gate.sh "$$tmp" && \
	QT_QUICK_BACKEND=rhi QSG_RHI_BACKEND=opengl QT_QPA_PLATFORM=offscreen qmltestrunner-qt6 -import "$$tmp" -input shell/gate/tests

gate-shots: shaders
	tmp=$$(mktemp -d) && trap 'rm -rf "$$tmp"' EXIT && scripts/assemble-gate.sh "$$tmp" && \
	QT_SCALE_FACTOR=1.15 QT_QUICK_BACKEND=rhi QSG_RHI_BACKEND=opengl QT_QPA_PLATFORM=offscreen qmltestrunner-qt6 -import "$$tmp" -input shell/gate/shots

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

smoke-lock: shaders
	scripts/smoke-lock.sh

smoke-login: shaders
	scripts/smoke-login.sh
