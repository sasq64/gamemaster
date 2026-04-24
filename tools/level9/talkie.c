/***********************************************************************\
*
* Level 9 interpreter
* Version 5.2
* Copyright (c) 1996-2025 Glen Summers and contributors.
* Contributions from David Kinder, Alan Staniforth, Simon Baldwin,
* Dieter Baron and Andreas Scherrer.
*
* This program is free software; you can redistribute it and/or modify
* it under the terms of the GNU General Public License as published by
* the Free Software Foundation; either version 2 of the License, or
* (at your option) any later version.
*
* This program is distributed in the hope that it will be useful,
* but WITHOUT ANY WARRANTY; without even the implied warranty of
* MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
* GNU General Public License for more details.
*
* You should have received a copy of the GNU General Public License
* along with this program; if not, write to the Free Software
* Foundation, Inc., 59 Temple Place, Suite 330, Boston, MA 02111, USA.
*
\***********************************************************************/

#include <ctype.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <sys/stat.h>
#include "level9.h"

#define TEXTBUFFER_SIZE 10240
char TextBuffer[TEXTBUFFER_SIZE + 1];
char* ptr = TextBuffer;

int Column = 0;
#define SCREENWIDTH 76

BitmapType bitmap_type = NO_BITMAPS;
const char* bitmap_dir = NULL;

void os_printchar(char c)
{
    if (ptr - TextBuffer >= TEXTBUFFER_SIZE) {
        os_flush();
    }
    if (c == 13) {
        *ptr++ = 10;
        os_flush();
    } else {
        *ptr++ = c;
    }
}


void dump_bitmap(int no)
{
    Bitmap* bitmap = DecodeBitmap(bitmap_dir, bitmap_type, no, 0, 0);
    if (bitmap) {
        printf("#[img %d %d %d %d]\n", no, bitmap->width, bitmap->height, bitmap->npalette);
        printf("#[pal %d", no);
        for (int i = 0; i < bitmap->npalette; i++) {
            printf(" 0x%02X%02X%02X", bitmap->palette[i].red, bitmap->palette[i].green, bitmap->palette[i].blue);
        }
        printf("]\n");
        printf("#[pixels %d", no);
        for (int i = 0; i < bitmap->width * bitmap->height; i++) {
            int pixel_index = bitmap->bitmap[i];
            printf(" 0x%02X", pixel_index); 
        }
        printf("]\n");
    }
}

static int key_mode = 0;

L9BOOL os_input(char* ibuff, int size)
{
    if (key_mode == 1) {
        key_mode = 0;
        puts("#[linemode]");
    }
    os_flush();
    fgets(ibuff, size, stdin);
    char* nl = strchr(ibuff, '\n');
    if (nl) *nl = 0;
    if (strncmp(ibuff, "##img#", 6) == 0) {
        int no = atoi(&ibuff[6]);
        dump_bitmap(no);
        return FALSE;
    }
    return TRUE;
}

char os_readchar(int millis)
{
    if (key_mode == 0) {
        key_mode = 1;
        puts("#[keymode]");
    }
    static int count = 0;
    char c;

    os_flush();
    if (millis == 0) return 0;

    /* Some of the Level 9 games expect to be able to wait for
       a character for a short while as a way of pausing, and
       expect 0 to be returned, while the multiple-choice games
       (such as The Archers) expect 'proper' keys from this
       routine.

       To get round this, we return 0 for the first 1024 calls,
       and 'proper' keys thereafter. Since The Archers and
       similar games ignore the returned zeros, this works quite
       well. A 'correct' port would solve this properly by
       implementing a timed wait for a key, but this is not
       possible using only C stdio-functions.
    */
    if (++count < 1024) return 0;
    count = 0;

    fprintf(stderr, "READCHAR\n");
    c = getc(stdin); /* will require enter key as well */
    fprintf(stderr, "GOT %02x\n", c);
    //if (c != '\n') {
    //    while (getc(stdin) != '\n') {
    //        /* unbuffer input until enter key */
    //    }
    //}

    return c;
}

L9BOOL os_stoplist(void)
{
    return FALSE;
}

void os_flush(void)
{
    *ptr = 0;
    fputs(TextBuffer, stdout);
    ptr = TextBuffer;
    fflush(stdout);
}

L9BOOL os_save_file(L9BYTE* Ptr, int Bytes)
{
    char name[256];
    char* nl;
    FILE* f;

    os_flush();
    printf("Save file: ");
    fgets(name, 256, stdin);
    nl = strchr(name, '\n');
    if (nl) *nl = 0;
    f = fopen(name, "wb");
    if (!f) return FALSE;
    fwrite(Ptr, 1, Bytes, f);
    fclose(f);
    return TRUE;
}

L9BOOL os_load_file(L9BYTE* Ptr, int* Bytes, int Max)
{
    char name[256];
    char* nl;
    FILE* f;

    os_flush();
    printf("Load file: ");
    fgets(name, 256, stdin);
    nl = strchr(name, '\n');
    if (nl) *nl = 0;
    f = fopen(name, "rb");
    if (!f) return FALSE;
    *Bytes = fread(Ptr, 1, Max, f);
    fclose(f);
    return TRUE;
}

L9BOOL os_get_game_file(char* NewName, int Size)
{
    char* nl;

    os_flush();
    printf("Load next game: ");
    fgets(NewName, Size, stdin);
    nl = strchr(NewName, '\n');
    if (nl) *nl = 0;
    return TRUE;
}


#if defined(_Windows) || defined(__MSDOS__) || defined(_WIN32) ||              \
    defined(__WIN32__)
#define PATH_SEP '\\'
#else
#define PATH_SEP '/'
#endif

void os_set_filenumber(char* NewName, int Size, int n)
{
    char* p;
    int i;

    char new_char = '0' + n;
    p = strrchr(NewName, PATH_SEP);
    if (p == NULL) p = NewName;
    for (i = strlen(p) - 1; i >= 0; i--) {
        if (isdigit(p[i])) {
            struct stat ss;
            char old_char = p[i];
            p[i] = new_char;
            if (stat(NewName, &ss) == 0) {
                return;
            }
            p[i] = old_char;
        }
    }

}

void os_graphics(int mode)
{
    printf("#[gfx %d]\n", mode);
    int width;
    int height;
    GetPictureSize(&width, &height);
    if (width != 0) {
        printf("#[imgsize %d %d]\n", width, height);
    }
}

void os_cleargraphics(void)
{
    printf("#[clear]\n");
}

void os_setcolour(int colour, int index)
{
    printf("#[setcolor %d %d]\n", colour, index);
}

void os_drawline(int x1, int y1, int x2, int y2, int colour1, int colour2)
{
    printf("#[line %d %d %d %d %d %d]\n", x1, y1, x2, y2, colour1, colour2);
}

void os_fill(int x, int y, int colour1, int colour2)
{
    printf("#[fill %d %d %d %d]\n", x, y, colour1, colour2);
}

static int used[64] = {0};

void os_show_bitmap(int pic, int x, int y)
{
    if (used[pic] == 0) {
        dump_bitmap(pic);
    }
    used[pic] = 1;
    printf("#[bitmap %d %d %d]\n", pic, x, y);
}

FILE* os_open_script_file(void)
{
    char name[256];
    char* nl;

    os_flush();
    printf("Script file: ");
    fgets(name, 256, stdin);
    nl = strchr(name, '\n');
    if (nl) *nl = 0;
    return fopen(name, "rt");
}

L9BOOL os_find_file(char* NewName)
{
    FILE* f = fopen(NewName, "rb");
    if (f != NULL) {
        fclose(f);
        return TRUE;
    }
    return FALSE;
}

int main(int argc, char** argv)
{
    printf("Level 9 Interpreter\n\n");
    if (!LoadGame(argv[1], NULL)) {
        printf("Error: Unable to open game file\n");
        return 0;
    }
    if (argc > 2) {
        bitmap_type = DetectBitmaps(argv[2]);
        printf("Type %d\n", bitmap_type);
        bitmap_dir = argv[2];
    }
    int rc = 1;
    while (rc) {
        rc = RunGame();
        int rg = 1;
        while (rg != 0) {
            rg = RunGraphics();
        }
    }
    StopGame();
    FreeMemory();
    return 0;
}
