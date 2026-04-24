/****************************************************************************\
*
* Magnetic - Magnetic Scrolls Interpreter.
*
* Written by Niclas Karlsson <nkarlsso@abo.fi>,
*            David Kinder <davidk@davidkinder.co.uk>,
*            Stefan Meier <Stefan.Meier@if-legends.org> and
*            Paul David Doherty <pdd@if-legends.org>
*
* Copyright (C) 1997-2023  Niclas Karlsson
*
*     This program is free software; you can redistribute it and/or modify
*     it under the terms of the GNU General Public License as published by
*     the Free Software Foundation; either version 2 of the License, or
*     (at your option) any later version.
*
*     This program is distributed in the hope that it will be useful,
*     but WITHOUT ANY WARRANTY; without even the implied warranty of
*     MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
*     GNU General Public License for more details.
*
*     You should have received a copy of the GNU General Public License
*     along with this program; if not, write to the Free Software
*     Foundation, Inc., 59 Temple Place, Suite 330, Boston, MA 02111, USA.
*
*     Simple ANSI interface main.c
*
\****************************************************************************/

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>
#include "defs.h"

#define WIDTH 78

uint8_t ms_gfx_enabled = 0;

// uint8_t buffer[80], xpos = 0, bufpos = 0, log_on = 0, ms_gfx_enabled,
//                filename[256];
// FILE *logfile1 = 0, *logfile2 = 0;

uint8_t ms_load_file(const char* name, uint8_t* ptr, uint16_t size)
{
    FILE* fh;
    const char* realname;

    if (name)
        realname = name;
    else {
        char filename[256];
        do {
            printf("Filename: ");
        } while (!fgets(filename, 256, stdin));
        filename[strlen(filename) - 1] = 0;
        realname = filename;
    }
    if (!(fh = fopen(realname, "rb"))) return 1;
    if (fread(ptr, 1, size, fh) != size) return 1;
    fclose(fh);
    return 0;
}

uint8_t ms_save_file(const char* name, uint8_t* ptr, uint16_t size)
{
    FILE* fh;
    const char* realname;

    if (name)
        realname = name;
    else {
        char filename[256];
        do {
            printf("Filename: ");
        } while (!fgets(filename, 256, stdin));
        filename[strlen(filename) - 1] = 0;
        realname = filename;
    }
    if (!(fh = fopen(realname, "wb"))) return 1;
    if (fwrite(ptr, 1, size, fh) != size) return 1;
    fclose(fh);
    return 0;
}

int log_on = 0;
FILE *logfile1 = 0, *logfile2 = 0;

void script_write(uint8_t c)
{
    if (log_on == 2 && fputc(c, logfile1) == EOF) {
        printf("[Problem with script file - closing]\n");
        fclose(logfile1);
        log_on = 0;
    }
}

void transcript_write(uint8_t c)
{
    if (logfile2 && c == 0x08 && ftell(logfile2) > 0)
        fseek(logfile2, -1, SEEK_CUR);
    else if (logfile2 && fputc(c, logfile2) == EOF) {
        printf("[Problem with transcript file - closing]\n");
        fclose(logfile2);
        logfile2 = 0;
    }
}

char status_left[80] = "";
char status_right[80] = "";
char status_side = 0;

void ms_statuschar(uint8_t c)
{ 
    if (c == 0xa) {
        char* ptr = &status_left[strlen(status_left)-1];
        while (*ptr == ' ') { *ptr-- = 0; };
        ptr = status_right;
        while (*ptr == ' ') { ptr++; };
        printf("#[status ::%s::%s::]\n", status_left, ptr);
        status_left[0] = 0;
        status_right[0] = 0;
        status_side = 0;

    } else if (c == 9) {
        status_side = 1;
    } else {
        char* ptr = status_side == 0 ? status_left : status_right;
        ptr = &ptr[strlen(ptr)];
        *ptr++ = c;
        *ptr = 0;
    }
}

char buffer[256];
int bufpos = 0;

void ms_flush(void)
{
    if (bufpos == 0) return;
    buffer[bufpos] = 0;
    fputs(buffer, stdout);
    bufpos = 0;
    fflush(stdout);
}

void ms_putchar(uint8_t c)
{
    if (c == 0x08) {
        if (bufpos > 0) bufpos--;
        return;
    }
    buffer[bufpos++] = c;
    if ((c == 0x0a) || (bufpos >= 200)) ms_flush();
}

uint8_t ms_getchar(uint8_t trans)
{
    static uint8_t buf[256];
    static uint16_t pos = 0;
    int c;
    uint8_t i;

    if (!pos) {
        /* Read new line? */
        i = 0;
        while (1) {
            if (log_on == 1) {
                /* Reading from logfile */
                if ((c = fgetc(logfile1)) == EOF) {
                    /* End of log? - turn off */
                    log_on = 0;
                    fclose(logfile1);
                    c = getchar();
                } else
                    printf("%c", c); /* print the char as well */
            } else {
                c = getchar();
                if (c == '#' && !i && trans) {
                    /* Interpreter command? */
                    while ((c = getchar()) != '\n' && c != EOF && i < 255)
                        buf[i++] = c;
                    buf[i] = 0;
                    c = '\n'; /* => Prints new prompt */
                    i = 0;
                    if (!strcmp(buf, "logoff") && log_on == 2) {
                        printf("[Closing script file]\n");
                        log_on = 0;
                        fclose(logfile1);
                    } else if (!strcmp(buf, "undo"))
                        c = 0;
                    else
                        printf("[Nothing done]\n");
                }
            }
            script_write((uint8_t)c);
            if (c != '\n') transcript_write((uint8_t)c);
            if (c == '\n' || c == EOF || i == 255) break;
            buf[i++] = c;
            if (!c) break;
        }
        buf[i] = '\n';
    }
    if ((c = buf[pos++]) == '\n' || !c) pos = 0;
    return (uint8_t)c;
}

void ms_showpic(uint32_t c, uint8_t mode)
{
    if (mode == 0) return; // graphics off

    uint16_t w, h, pal[16];
    uint8_t* bitmap = ms_extract(c, &w, &h, pal, 0);

    if (bitmap) {
        printf("#[img %d %d %d %d]\n", c, w, h, 16);
        printf("#[pal %d", c);
        for (int i = 0; i < 16; i++) {
            // Convert 3-bit RGB to 8-bit RGB (palette format: 00000RRR0GGG0BBB)
            uint8_t red = ((pal[i] >> 8) & 0x07) * 36;   // scale 0-7 to 0-252
            uint8_t green = ((pal[i] >> 4) & 0x07) * 36; // scale 0-7 to 0-252
            uint8_t blue = (pal[i] & 0x07) * 36;         // scale 0-7 to 0-252
            printf(" 0x%02X%02X%02X", red, green, blue);
        }
        printf("]\n");
        printf("#[pixels %d", c);
        for (int i = 0; i < w * h; i++) {
            printf(" 0x%02X", bitmap[i]);
        }
        printf("]\n");
        printf("#[bitmap %d]\n", c);
    }
}

void ms_fatal(const char* txt)
{
    fputs("\nFatal error: ", stderr);
    fputs(txt, stderr);
    fputs("\n", stderr);
    ms_status();
    exit(1);
}

uint8_t ms_showhints(struct ms_hint* hints)
{
    return 0;
}

void ms_playmusic(uint8_t* midi_data, uint32_t length, uint16_t tempo) {}

int main(int argc, char** argv)
{
    uint8_t running, i, *gamename = 0, *gfxname = 0, *hintname = 0;
    uint32_t dlimit, slimit;

    if (sizeof(uint8_t) != 1 || sizeof(uint16_t) != 2 ||
        sizeof(uint32_t) != 4) {
        fprintf(stderr,
                "Unsupported platform: stdint types have unexpected sizes\n");
        exit(1);
    }
    dlimit = slimit = 0xffffffff;
    for (i = 1; i < argc; i++) {
        if (argv[i][0] == '-') {
            switch (tolower(argv[i][1])) {
            case 'd':
                if (strlen(argv[i]) > 2)
                    dlimit = atoi(&argv[i][2]);
                else
                    dlimit = 0;
                break;
            case 's':
                if (strlen(argv[i]) > 2)
                    slimit = atoi(&argv[i][2]);
                else
                    slimit = 655360;
                break;
            case 't':
                if (!(logfile2 = fopen(&argv[i][2], "w")))
                    printf("Failed to open \"%s\" for writing.\n", &argv[i][2]);
                break;
            case 'r':
                if (logfile1 = fopen(&argv[i][2], "r"))
                    log_on = 1;
                else
                    printf("Failed to open \"%s\" for reading.\n", &argv[i][2]);
                break;
            case 'w':
                if (logfile1 = fopen(&argv[i][2], "w"))
                    log_on = 2;
                else
                    printf("Failed to open \"%s\" for writing.\n", &argv[i][2]);
                break;
            default:
                printf("Unknown option -%c, ignoring.\n", argv[i][1]);
                break;
            }
        } else if (!gamename)
            gamename = argv[i];
        else if (!gfxname)
            gfxname = argv[i];
        else if (!hintname)
            hintname = argv[i];
    }
    if (!gamename) {
        printf("Magnetic 2.3.1 - a Magnetic Scrolls interpreter\n\n");
        printf(
            "Usage: %s [options] game [gfxfile] [hintfile]\n\n"
            "Where the options are:\n"
            " -dn    activate register dump (after n instructions)\n"
            " -rname read script file\n"
            " -sn    safety mode, exits automatically (after n instructions)\n"
            " -tname write transcript file\n"
            " -wname write script file\n\n"
            "The interpreter commands are:\n"
            " #undo   undo - don't use it near are_you_sure prompts\n"
            " #logoff turn off script writing\n\n",
            argv[0]);
        exit(1);
    }

    if (!(ms_gfx_enabled = ms_init(gamename, gfxname, hintname, 0))) {
        printf("Couldn't start up game \"%s\".\n", gamename);
        exit(1);
    }
    ms_gfx_enabled--;
    running = 1;
    while ((ms_count() < slimit) && running) {
        if (ms_count() >= dlimit) ms_status();
        running = ms_rungame();
    }
    if (ms_count() == slimit) {
        printf("\n\nSafety limit (%d) reached.\n", slimit);
        ms_status();
    }
    ms_freemem();
    if (log_on) fclose(logfile1);
    if (logfile2) fclose(logfile2);
    printf("\nExiting.\n");
    return 0;
}
