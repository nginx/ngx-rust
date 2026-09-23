TEST_NGINX_GLOBALS	+= $(LOAD_CARGO_BUILT_MODULE)

NGINX_CONFIGURE_ARGS	+= \
	--with-debug \
	--add-dynamic-module="$(MODULE_SOURCE_DIR)"

# Use separate build directory for instrumented objects
CARGO_PROFILE		= coverage
CARGO_TARGET_SUBDIR	= target/$(HOST_TUPLE)/$(CARGO_PROFILE)

# Profile format may change between LLVM versions.
# The tools specified here must be compatible with the format emitted by rustc.
#
# One approach is to check the LLVM version used by rustc (rustc -vV|grep LLVM)
# and download or install matching binaries, another is to use
# distribution-provided Rust toolchain built with system LLVM.
LLVM_COV	?= llvm-cov
# It is recommended to use rustfilt, but recent enough llvm-cxxfilt
# should fully support Rust v0 mangling scheme
LLVM_COV_DEMANGLER ?= llvm-cxxfilt
LLVM_PROFDATA	?= llvm-profdata

LLVM_COV_FLAGS	= \
	-Xdemangler=$(LLVM_COV_DEMANGLER) \
	-ignore-filename-regex='/(registry|library|target)/' \
	-instr-profile=$(NGINX_BUILD_DIR)/coverage.profdata \
	$(foreach M,$(CARGO_BUILT_MODULE),-object $(M))

LLVM_PROFILE_FILE = $(NGINX_BUILD_DIR)/%m_%p.profraw

RUSTFLAGS	+= -C instrument-coverage
# GNU or LLVM ld; ignore unresolved references to nginx symbols
RUSTFLAGS	+= -C link-arg=-Wl,--unresolved-symbols=ignore-in-object-files

# Unstable flags for branch statistics (rust-lang/rust#79649)
RUSTFLAGS	+= -Z coverage-options=branch
BUILD_ENV	+= RUSTC_BOOTSTRAP=1

# Do not apply RUSTFLAGS to buildscripts and procedural macros (--target)
BUILD_ENV	+= CARGO_BUILD_TARGET="$(HOST_TUPLE)"
BUILD_ENV	+= RUSTFLAGS="$(RUSTFLAGS)"
BUILD_ENV	+= LLVM_PROFILE_FILE="$(LLVM_PROFILE_FILE)"
TEST_ENV	+= LLVM_PROFILE_FILE="$(LLVM_PROFILE_FILE)"

build: $(CARGO_BUILT_MODULE)


.PHONY: coverage-clean coverage-html coverage-lcov coverage-summary

coverage-clean: ## Clean old coverage data and force recollection
	rm -f $(NGINX_BUILD_DIR)/.profraw $(NGINX_BUILD_DIR)/*.profraw

coverage-html: ## Generate code coverage report in HTML format
coverage-html: $(NGINX_BUILD_DIR)/coverage-html

coverage-lcov: ## Generate code coverage report in lcov format
coverage-lcov: $(NGINX_BUILD_DIR)/coverage.lcov

coverage-summary: ## Print code coverage summary
coverage-summary: $(NGINX_BUILD_DIR)/coverage.profdata
	$(LLVM_COV) report $(LLVM_COV_FLAGS) -use-color


$(NGINX_BUILD_DIR)/.profraw: $(TEST_NGINX_BINARY) $(CARGO_BUILT_MODULE)
	-rm -f $(NGINX_BUILD_DIR)/*.profraw
	# inlined to avoid recursive variable expansion when calling submake
	$(PROVE) --state=save -j $(TEST_JOBS) $(TESTS) || \
		$(PROVE) --state=failed -v
	$(BUILD_ENV) $(TEST_ENV) $(NGX_CARGO) test $(CARGO_PROFILE_ARG)
	touch $@

$(NGINX_BUILD_DIR)/coverage.profdata: $(NGINX_BUILD_DIR)/.profraw
	$(LLVM_PROFDATA) merge -sparse -o $@ $(NGINX_BUILD_DIR)/*.profraw

$(NGINX_BUILD_DIR)/coverage.lcov: $(NGINX_BUILD_DIR)/coverage.profdata
	$(LLVM_COV) export $(LLVM_COV_FLAGS) -format=lcov > $@

$(NGINX_BUILD_DIR)/coverage-html: $(NGINX_BUILD_DIR)/coverage.profdata
	$(LLVM_COV) show $(LLVM_COV_FLAGS) \
		-format=html \
		-output-dir=$@ \
		-show-branches=count \
		-show-expansions \
		-show-instantiations \
		-show-line-counts-or-regions
