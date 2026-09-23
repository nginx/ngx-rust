CFLAGS_ASAN	+= -O1 -fsanitize=address -fno-omit-frame-pointer
CFLAGS_ASAN	+= -DNGX_DEBUG_PALLOC=1 -DNGX_SUPPRESS_WARN=1
LDFLAGS_ASAN	+= -fsanitize=address

RUSTFLAGS 	+= -Cforce-frame-pointers=yes
RUSTFLAGS 	+= -Zsanitizer=address -Zexternal-clangrt

BUILD_ENV	+= RUSTFLAGS="$(RUSTFLAGS)"
BUILD_ENV	+= RUSTC_BOOTSTRAP=1
BUILD_ENV	+= NGX_RUSTC_OPT="-Zbuild-std"
BUILD_ENV	+= NGX_RUST_TARGET="$(HOST_TUPLE)"

TEST_ENV	+= ASAN_OPTIONS=detect_stack_use_after_return=1:detect_odr_violation=0
TEST_ENV	+= LSAN_OPTIONS="suppressions=$(CURDIR)/build/lsan-suppressions.txt"
TEST_ENV	+= TEST_NGINX_CATLOG=1

NGINX_CONFIGURE_ARGS	+= \
	--with-cc=clang \
	--with-cc-opt="$(CFLAGS_ASAN)" \
	--with-ld-opt="$(LDFLAGS_ASAN)" \
	--with-debug \
	--add-module="$(MODULE_SOURCE_DIR)"
