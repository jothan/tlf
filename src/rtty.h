/*
 * Tlf - contest logging program for amateur radio operators
 * Copyright (C) 2001-2002-2003-2004 Rein Couperus <pa0r@eudxf.org>
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
 * Foundation, Inc., 59 Temple Place, Suite 330, Boston, MA  02111-1307  USA
 */

  	/* ------------------------------------------------------------
 	*      rtty.h   rtty  mini terminal
 	*
 	*--------------------------------------------------------------*/

#ifndef RTTY_H

#define RTTY_H

int init_controller() ;
void deinit_controller();
int rx_rtty () ;
int show_rtty(void);
int get_last_rtty_line(char * line);

#endif /* end of include guard: RTTY_H */

