# kestrel quality gate and install. Builds run at background priority so the desktop stays smooth.
SCOPE = systemd-run --user --scope -q -p CPUWeight=1 -p IOWeight=1 --

.PHONY: check build install restart dev

check:
	cargo fmt --all -- --check
	$(SCOPE) cargo clippy --release --workspace --all-targets -- -D warnings
	$(SCOPE) cargo test --release --workspace

build:
	$(SCOPE) cargo build --release

# Package, install and restart the running shell. Refuses to install if the checks fail.
install: check
	cd packaging/arch && $(SCOPE) makepkg -f --noconfirm >/dev/null && sudo pacman -U --noconfirm kestrel-*.pkg.tar.zst >/dev/null && rm -rf kestrel-*.pkg.tar.zst pkg src
	systemctl --user restart kestrel

# Dev mode: the shell runs from this tree and restarts on every save that builds. A failing
# build keeps the running shell; Ctrl+C hands the desktop back to the installed kestrel.
dev:
	@systemctl --user stop kestrel; pid=; \
	trap 'kill $$pid 2>/dev/null; systemctl --user start kestrel; exit 0' INT TERM; \
	while true; do \
		stamp=$$(mktemp); \
		if $(SCOPE) cargo build --release 2>&1 | grep -E '^(error|warning)' -A6; then \
			echo "dev: build has problems; the running shell is kept"; \
		fi; \
		if [ target/release/kestrel -nt $$stamp ] || [ -z "$$pid" ]; then \
			kill $$pid 2>/dev/null; wait $$pid 2>/dev/null; \
			target/release/kestrel & pid=$$!; echo "dev: kestrel restarted ($$pid)"; \
		fi; \
		until [ -n "$$(find src crates vendor assets Cargo.toml -newer $$stamp -type f 2>/dev/null | head -1)" ]; do sleep 1; done; \
		rm -f $$stamp; \
	done
