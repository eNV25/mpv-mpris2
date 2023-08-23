
export prefix := "/usr/local"
export config_system := if clean(prefix) == "/usr" {
	"/etc"
} else if clean(prefix) == "/" {
	"/etc"
} else {
	"/usr/local/etc"
}
export config_user := join(env_var("HOME"), ".config")

set shell := ["sh", "-xc"]

default:
	@just --choose

build *args="":
	cargo build {{args}}

install: (build "--release")
	install -v -D target/release/libmpv_mpris2.so "${prefix}/lib/mpv-mpris2/mpris.so"
	mkdir -p "${config_system}/mpv/scripts/"
	ln -v -s -t "${config_system}/mpv/scripts/" "${prefix}/lib/mpv-mpris2/mpris.so"

uninstall:
	rm "${config_system}/mpv/scripts/mpris.so"
	rm -rf "${prefix}/lib/mpv-mpris2/"

install-user: (build "--release")
	install -v -D target/release/libmpv_mpris2.so "${config_user}/mpv/scripts/mpris.so"

uninstall-user:
	rm "${config_user}/mpv/scripts/mpris.so"
