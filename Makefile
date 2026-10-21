SHADER_DIR := shell/kit/GhostKit/shaders
SHADERS := $(wildcard $(SHADER_DIR)/*.frag)
COMPILED_SHADERS := $(SHADERS:.frag=.frag.qsb)

.PHONY: shaders test smoke

shaders: $(COMPILED_SHADERS)

%.frag.qsb: %.frag
	qsb-qt6 --qt6 -o $@ $<

test:
	node --test 'shell/tests/*.test.mjs'
	cargo test --workspace

smoke: shaders
	scripts/qml-smoke.sh
