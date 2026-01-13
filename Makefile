# kestrel quality gate and install. Builds run at background priority so the desktop stays smooth.
SCOPE = systemd-run --user --scope -q -p CPUWeight=1 -p IOWeight=1 --

.PHONY: check build install restart

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
