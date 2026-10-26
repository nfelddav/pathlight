# Builds the C example against the static library from `cargo build --release`.
ROOT := ../..
CFLAGS ?= -std=c99 -Wall -Wextra -Werror -O2
LDLIBS := $(ROOT)/target/release/libpathlight.a -lpthread -ldl -lm

hello: hello.c $(ROOT)/crates/pathlight-capi/include/pathlight.h $(ROOT)/target/release/libpathlight.a
	$(CC) $(CFLAGS) -I$(ROOT)/crates/pathlight-capi/include -o $@ hello.c $(LDLIBS)

$(ROOT)/target/release/libpathlight.a:
	cargo build --release -p pathlight-capi

run: hello
	./hello

clean:
	rm -f hello

.PHONY: run clean
