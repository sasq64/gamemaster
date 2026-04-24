# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

ZTools is a collection of C utilities for analyzing and manipulating Infocom Z-machine story files (interactive fiction games). This is a classic C codebase originally by Mark Howell, currently maintained by Matthew T. Russotto.

## Build Commands

**Primary build system**: Unix Make
```bash
make                    # Build all tools (check, infodump, pix2gif, txd)
make clean             # Remove object files and executables
make doc               # Generate formatted man pages
```

**Platform-specific makefiles available**:
- `beos.mak` - BeOS
- `msc.mak` - Microsoft Quick C (DOS: `nmake /F msc.mak`)
- `Makefile.djpp` - DJGPP (DOS)
- `amiga.mak` - Amiga Dice C
- `aztec.mak` - Amiga Aztec C
- `mpwcw.make` - Macintosh Code Warrior C

## Core Architecture

**Main utilities**:
- **check**: Story file integrity checker (`check story-file [new-file]`)
- **infodump**: Header/object/dictionary analyzer (`infodump [options] story-file`)
- **txd**: Z-code disassembler (`txd [options] story-file`)
- **pix2gif**: Picture converter for V6 games (`pix2gif picture-file`)
- **inforead**: IBM bootable disk converter (PC-only, `inforead story-file [track] [drive]`)

**Shared infrastructure**:
- `tx.h` - Common data structures and Z-machine definitions
- `txio.c` - Story file I/O routines
- `getopt.c` - Command line option parsing
- `infinfo.c` - Story file information extraction
- `symbols.c` - Symbol table handling (supports Inform symbols with `-s` flag)

**Key data structures**:
- `zheader_t` - Z-machine file header format
- `zobjectv3_t`/`zobjectv4_t` - Object formats for different versions
- Version support: V1-V8 games, with special handling for V6 graphics

**Important notes**:
- Uses custom Z-machine types: `zbyte_t` (unsigned char), `zword_t` (unsigned short)
- Supports both Infocom and Inform assembly syntax
- Symbol tables enhance disassembly readability when available
- Cross-platform compatibility with various C compilers and systems