.DEFAULT_GOAL := help

export PATH := $(HOME)/.cargo/bin:$(PATH)
CARGO ?= $(shell PATH="$(HOME)/.cargo/bin:$$PATH" command -v cargo 2>/dev/null || echo cargo)

BIN_NAME := zhihu
BIN := target/release/$(BIN_NAME)

.PHONY: help build test fmt clippy check clean

help: ## 显示帮助
	@echo "用法: make <目标>"
	@echo
	@echo "可用目标:"
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z0-9_-]+:.*## / {printf "  %-10s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

build: ## 构建 release，并把二进制复制到当前目录
	$(CARGO) build --release
	cp -f $(BIN) ./$(BIN_NAME)
	@echo "已复制到: ./$(BIN_NAME)"

test: ## 运行单元测试
	$(CARGO) test

fmt: ## 格式化代码
	$(CARGO) fmt

clippy: ## 运行 clippy 检查
	$(CARGO) clippy --all-targets

check: ## 检查格式、clippy 和测试
	$(CARGO) fmt --check
	$(CARGO) clippy --all-targets
	$(CARGO) test

clean: ## 删除构建产物和当前目录的二进制
	$(CARGO) clean
	rm -f ./$(BIN_NAME)