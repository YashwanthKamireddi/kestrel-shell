# kestrel quality gate and install. Builds run at full speed (his call 2026-11-14: never low priority).
SCOPE =

.PHONY: check build install restart dev

check:
	cargo fmt --all -- --check
	tools/lint/tokens.sh
	# kestrel's own crates, by name: --workspace also took in the vendored iced_layershell (a
	# path dependency is a member automatically) with its default features, and so checked
	# kestrel with the GPU renderer compiled in, a different program from the one installed.
	$(SCOPE) cargo clippy --release -p kestrel -p kestrel-services --all-targets -- -D warnings
	$(SCOPE) cargo test --release -p kestrel -p kestrel-services

build:
	$(SCOPE) cargo build --release

# Package, install and restart the running shell. Refuses to install if the checks fail.
# The binary is built once, here, in the same environment as the checks (makepkg reuses it), and
# the tests are not run a second time (--nocheck: `check` just ran them).
install: check
	$(SCOPE) cargo build --release
	cd packaging/arch && $(SCOPE) makepkg -f --noconfirm --nocheck --noextract >/dev/null && sudo pacman -U --noconfirm kestrel-shell-*.pkg.tar.zst >/dev/null && rm -rf kestrel-shell-*.pkg.tar.zst pkg src
	# makepkg stamps the version into PKGBUILD; put the committed line back (only that line).
	sed -i "s/^pkgver=.*/$$(git show HEAD:packaging/arch/PKGBUILD | grep '^pkgver=')/" packaging/arch/PKGBUILD
	# Both processes run the new binary (the prompt process is its own unit, src/prompt.rs).
	systemctl --user daemon-reload
	systemctl --user restart kestrel kestrel-prompt

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
