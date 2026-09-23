HOST_TUPLE	:= $(shell $(NGX_CARGO) -vV | awk '/^host: / { print $$2; }')
TEST_JOBS	:= $(shell nproc 2>/dev/null || getconf NPROCESSORS_ONLN 2>/dev/null || echo 1)

# extension for Rust cdylib targets
ifeq ($(shell uname), Darwin)
SHLIB_EXT	= .dylib
else
SHLIB_EXT	= .so
endif

# resolve paths

NGINX_SOURCE_DIR	:= $(shell CDPATH='' cd $(NGINX_SOURCE_DIR) && pwd)
NGINX_TESTS_DIR		:= $(shell CDPATH='' cd $(NGINX_TESTS_DIR) && pwd)
NGINX_BUILD_DIR		:= $(shell CDPATH='' mkdir -p $(NGINX_BUILD_DIR) && cd $(NGINX_BUILD_DIR) && pwd)
