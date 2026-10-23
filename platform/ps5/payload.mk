# SPDX-License-Identifier: GPL-3.0-or-later
# Fixed minimal runtime only. No stock SDK CRT, live adapter or deployment.
.DEFAULT_GOAL := all
.PHONY: all
all:
	python3 tools/build_ps5.py
