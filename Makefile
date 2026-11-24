# Requires Zig, cargo-zigbuild, and `rustup target add x86_64-unknown-linux-gnu`.
# Deployment uses rsync with --chown support and a privileged SSH login on home.
LINUX_TARGET := x86_64-unknown-linux-gnu
LINUX_GLIBC := 2.36
LINUX_BINARY := target/$(LINUX_TARGET)/release/risco2mqtt

.PHONY: build-linux deploy

build-linux:
	cargo zigbuild --locked --release --bin risco2mqtt --target $(LINUX_TARGET).$(LINUX_GLIBC) --target-dir target

deploy: build-linux
	cargo test --locked
	rsync --checksum --perms --chmod=0755 --owner --group --chown=alarmcore:alarmcore \
		$(LINUX_BINARY) home:/opt/risco2mqtt/bin/risco2mqtt
	ssh home 'systemctl restart risco2mqtt && systemctl is-active risco2mqtt'
	git push
