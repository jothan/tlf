/*
 * Tlf - contest logging program for amateur radio operators
 * Copyright (C) 2001-2002-2003 Rein Couperus <pa0rct@amsat.org>
 *
 * This program is free oftware; you can redistribute it and/or modify
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
 * Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301 USA
 */


#ifndef GETTXINFO_H
#define GETTXINFO_H

#define SETCWMODE   (-1)
#define SETSSBMODE  (-2)
#define RESETRIT    (-3)
#define SETDIGIMODE (-4)

#include <hamlib/rig.h>

void set_outfreq(freq_t hertz);
freq_t get_outfreq();

void gettxinfo(void);
void display_cw_speed(unsigned int wpm);

#endif /* GETTXINFO_H */
