MACHINE := $(shell getconf LONG_BIT)
ifeq ($(MACHINE), 64)
LIB = lib64
else
LIB = lib
endif

ifdef MINGW_PREFIX
export MSYS2_ARG_CONV_EXCL=*
export PREFIX=$(MINGW_PREFIX)
else
export PREFIX=/usr
endif

HEADER = inst/$(PREFIX)/include/rdw4/rdw-vnc.h
GIR = inst/$(PREFIX)/share/gir-1.0/RdwVnc-4.0.gir
TYPELIB = inst/$(PREFIX)/$(LIB)/girepository-1.0/RdwVnc-4.0.typelib
VAPI = inst/$(PREFIX)/share/vala/vapi/rdw4-vnc.vapi

RUST_SOURCES = $(shell find src)

all: $(GIR) $(TYPELIB) $(VAPI)

export PKG_CONFIG_PATH+=:$(PWD)/inst/$(PREFIX)/$(LIB)/pkgconfig
export GI_TYPELIB_PATH+=:$(PWD)/inst/$(PREFIX)/$(LIB)/girepository-1.0
export LD_LIBRARY_PATH=$(PWD)/inst/$(PREFIX)/$(LIB)

$(HEADER): $(RUST_SOURCES)
	cargo cinstall --release --destdir=inst --prefix=$(PREFIX) --libdir=$(PREFIX)/$(LIB)

$(GIR): $(HEADER)
	mkdir -p $(@D)
	g-ir-scanner -v --warn-all \
		--namespace RdwVnc --nsversion=4.0 \
		--identifier-prefix RdwVnc \
		-Iinst/include --c-include "rdw-vnc.h" \
		--library=rdw4-vnc --library-path=inst/$(PREFIX)/$(LIB) \
		--include=GVnc-1.0 --pkg gvnc-1.0 \
		--include=Rdw-4.0 --pkg rdw4 \
		--output $@ \
		$<

$(TYPELIB): $(GIR)
	mkdir -p $(@D)
	g-ir-compiler $< -o $@

$(VAPI): $(GIR)
	mkdir -p $(@D)
	vapigen \
		--pkg gvnc-1.0 \
		--pkg rdw4 \
		--library rdw4-vnc \
		$< -d $(@D)
	echo rdw4 > $(@D)/rdw4-vnc.deps
	echo gvnc-1.0 >> $(@D)/rdw4-vnc.deps

install: $(HEADER)
	cp -r inst/* $(DESTDIR)/
