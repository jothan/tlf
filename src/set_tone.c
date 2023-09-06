/*
 * Tlf - contest logging program for amateur radio operators
 * Copyright (C) 2001-2002-2003 Rein Couperus <pa0rct@amsat.org>
 *               2012           Thomas Beierlein <tb@forth-ev.de>
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
 * Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA
 */
/* ------------------------------------------------------------
 *        Set CW sidetone
 *
 *--------------------------------------------------------------*/


#include <stdlib.h>

#include "err_utils.h"
#include "globalvars.h"
#include "nicebox.h"	// Includes curses.h
#include "set_tone.h"
#include "tlf.h"
#include "rust.h"

void set_tone(void) {
    char tonestr[5] = "";

    if (trxmode != CWMODE)
	return;

    nicebox(4, 40, 1, 6, "Tone");
    attron(COLOR_PAIR(C_LOG) | A_STANDOUT);
    mvaddstr(5, 41, "      ");
    move(5, 42);
    echo();
    getnstr_process(tonestr, 3);
    noecho();
    tonestr[3] = '\0';

    write_tone(parse_tone(tonestr));
}
