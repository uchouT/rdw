CARGO_FLAGS ?= --release

MACHINE := $(shell getconf LONG_BIT)
ifeq ($(MACHINE), 64)
LIB ?= lib64
else
LIB ?= lib
endif

ifdef MINGW_PREFIX
export MSYS2_ARG_CONV_EXCL=*
export PREFIX=$(MINGW_PREFIX)
else
export PREFIX=/usr
endif

HEADER = inst/$(PREFIX)/include/rdw4/rdw-qemu.h
GIR = inst/$(PREFIX)/share/gir-1.0/RdwQemu-4.0.gir
TYPELIB = inst/$(PREFIX)/$(LIB)/girepository-1.0/RdwQemu-4.0.typelib
VAPI = inst/$(PREFIX)/share/vala/vapi/rdw4-qemu.vapi

RUST_SOURCES = $(shell find src -name '*.rs')

all: $(GIR) $(TYPELIB) $(VAPI)

export PKG_CONFIG_PATH+=:$(PWD)/inst/$(PREFIX)/$(LIB)/pkgconfig
export GI_TYPELIB_PATH+=:$(PWD)/inst/$(PREFIX)/$(LIB)/girepository-1.0
export LD_LIBRARY_PATH=$(PWD)/inst/$(PREFIX)/$(LIB)

$(HEADER): $(RUST_SOURCES)
	cargo cinstall $(CARGO_FLAGS) --destdir=inst --prefix=$(PREFIX) --libdir=$(PREFIX)/$(LIB)

$(GIR): $(HEADER)
	mkdir -p $(@D)
	g-ir-scanner -v --warn-all \
		--namespace RdwQemu --nsversion=4.0 \
		--identifier-prefix RdwQemu \
		--symbol-prefix rdw_qemu \
		--c-include "rdw-qemu.h" \
		-I$(PREFIX)/include \
		-Iinst/$(PREFIX)/include \
		-L$(PREFIX)/$(LIB) \
		-Linst/$(PREFIX)/$(LIB) \
		--library=rdw4-qemu \
		--include=Rdw-4.0 --pkg rdw4 \
		--output $@ \
		$<

$(TYPELIB): $(GIR)
	mkdir -p $(@D)
	g-ir-compiler $< -o $@

$(VAPI): $(GIR)
	mkdir -p $(@D)
	vapigen \
		--pkg rdw4 \
		--library rdw4-qemu \
		$< -d $(@D)
	echo rdw4 > $(@D)/rdw4-qemu.deps

install: all
	cp -r inst/* $(DESTDIR)/
