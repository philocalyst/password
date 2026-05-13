ROOT     := $(shell pwd)
PROFILE  ?= debug
RUST_OUT := $(ROOT)/target/$(PROFILE)/libpassword.dylib

FFI_SWIFT  := $(ROOT)/frontend/Sources/frontend/password.swift
FFI_HEADER := $(ROOT)/frontend/Sources/passwordFFI/include/passwordFFI.h
FFI_MAP    := $(ROOT)/frontend/Sources/passwordFFI/include/module.modulemap

BINDGEN_FLAGS := \
	generate \
	--library $(ROOT)/target/$(PROFILE)/libpassword.dylib \
	--language swift \
	--out-dir /tmp/uniffi-out

.PHONY: all build gen-bindings build-swift run clean fmt

all: build-swift

# 1. Build the Rust cdylib (and rlib for the CLI binary).
build:
ifeq ($(PROFILE),release)
	cargo build --release
else
	cargo build
endif

# 2. Run uniffi-bindgen and stage the generated files.
gen-bindings: build
	mkdir -p /tmp/uniffi-out \
	          $(ROOT)/frontend/Sources/passwordFFI/include
	cargo run $(if $(filter release,$(PROFILE)),--release) \
		--bin uniffi-bindgen -- $(BINDGEN_FLAGS)
	cp /tmp/uniffi-out/password.swift      $(FFI_SWIFT)
	cp /tmp/uniffi-out/passwordFFI.h       $(FFI_HEADER)
	cp /tmp/uniffi-out/passwordFFI.modulemap $(FFI_MAP)

# 3. Build the Swift frontend, supplying the dylib search path at link time.
build-swift: gen-bindings
	cd frontend && swift build \
		-Xlinker -L$(ROOT)/target/$(PROFILE) \
		-Xlinker -rpath \
		-Xlinker $(ROOT)/target/$(PROFILE)

# 4. Run the Swift frontend (builds first if needed).
run: gen-bindings
	cd frontend && swift run \
		-Xlinker -L$(ROOT)/target/$(PROFILE) \
		-Xlinker -rpath \
		-Xlinker $(ROOT)/target/$(PROFILE)

fmt:
	cargo fmt

clean:
	cargo clean
	rm -f $(FFI_SWIFT) $(FFI_HEADER) $(FFI_MAP)
	rm -rf /tmp/uniffi-out
	cd frontend && swift package clean
