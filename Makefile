# Windows GNU Make entry point. The PowerShell runner also works without Make.
SHELL := cmd.exe
.DEFAULT_GOAL := all
.NOTPARALLEL:
.PHONY: all setup check test build package smoke live help

all setup check test build package smoke live:
	powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/build.ps1 -Task $@

help:
	@echo make all      - setup, verify, build installer, installed smoke, artifacts
	@echo make setup    - check prerequisites, install locked dependencies and runtimes
	@echo make check    - TypeScript, Svelte and runtime integrity
	@echo make test     - all deterministic verification
	@echo make build    - production frontend and Windows installer
	@echo make package  - build installer and collect checksums/notices
	@echo make smoke    - test the installed package
	@echo make live     - real YouTube download and rollback tests
